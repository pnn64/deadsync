# WAV mutation and sanitizer validation

The shared property harness is `tests/fuzz_support/mod.rs`. It runs as an ordinary
Rust test, a portable deterministic mutation example, and an isolated libFuzzer
target. The normal library dependency graph remains empty. `libfuzzer-sys` 0.4.13
is pinned only in the separate `fuzz/` workspace and its lockfile.

## Inputs and properties

`tests/fuzz_corpus.py` constructs 162 seeds without calling this crate's writers
or a reference codec: 96 native WAV forms, 12 ADPCM files, 12 ADPCM blocks,
18 metadata payloads and 24 writer inputs. Container seeds span RIFF/RIFX/RF64/
BW64, both byte orders, six native widths, 1/2/3/33 channels, extensible valid
bits, float NaN payloads, data before format, trailing metadata, odd padding and
repeated ds64 table IDs. Direct metadata seeds keep parsers reachable when a
mutation destroys the outer RIFF signature.

Checks include:

- borrowed/file reader acceptance, format, fact/ds64 count and chunk agreement;
- forward-only chunk traversal through 1..31-byte reads and recurring Interrupted
  errors, including automatic payload/padding drainage;
- caller-buffer PCM integers and exact float bits, direct typed views, raw frame
  ranges, midpoint/end/invalid/overflowing seek positions;
- borrowed and cached ADPCM decoding/range agreement, block geometry, output
  capacity errors and native block encoder/decoder state transitions;
- metadata iterators, all exposed fields and canonical serialization properties,
  including untouched output on rejected BEXT/CHNA/marker writes;
- raw native encoding across all four containers, arbitrary frame splits, valid
  bits and exact payload roundtrips.

ADPCM comparisons inside this harness share a decoder implementation. They test
API/state agreement, not independent arithmetic correctness; the external oracle
campaigns in ADPCM.md supply separate evidence. Lossy encoding is never asserted
to reproduce the original samples. LIST padding and absent optional SXML alignment
tables may canonicalize, so their properties compare the corresponding fields
rather than requiring identical reconstructed payload bytes.

Input size is capped at 32 KiB. Parsing uses 64 chunks/metadata entries, 16 ds64
rows and 4096 format bytes. Compressed processing uses at most 4096 encoded bytes
and 8192 decoded samples per block. Larger valid parameters are deliberately
skipped by the harness; this is a test budget, not a production format limit.
The integration test also mutates each byte and every truncation of a small native
WAV and runs the same container/stream/seek agreement checks.

## Reproduction

Portable campaign (new output stem required):

```sh
python crates/deadlib-audio/decode/wav/tests/run_mutation.py --iterations 2000000 --output target/wav/mutation
python crates/deadlib-audio/decode/wav/tests/run_mutation.py --iterations 2000000 --seed 534932719 --target i686-pc-windows-msvc --output target/wav/mutation-i686
```

This runner uses xorshift64 and deletion, insertion, truncation, bit flips,
boundary-byte substitution, zero/FF runs and byte replacement. It replays every
original seed first. Build-only environment overrides enable overflow checks,
debug assertions and panic unwinding despite the main workspace's release abort
profile. A panic saves the current input and prints iteration, seed file and PRNG
state. Counts identify successful parser families; they are not code coverage.

Coverage-guided campaign with AddressSanitizer (install the tool into task-local
storage, preserving the user's global Cargo installation):

```sh
cargo install cargo-fuzz --version 0.13.2 --locked --root target/wav/fuzz-tools
python crates/deadlib-audio/decode/wav/tests/run_libfuzzer.py --runs 2000000 --output target/wav/asan --cargo-fuzz target/wav/fuzz-tools/bin/cargo-fuzz.exe
```

The script uses the installed nightly toolchain, cargo-fuzz's AddressSanitizer,
debug assertion and overflow-check defaults, the checked-in dictionary, a fresh
corpus and separate artifacts directory. On Windows it discovers the installed
MSVC ASan runtime with vswhere and adds its directory only to the subprocess PATH.
Record the compiler/runtime versions and hashes in each run's JSON. The
[Rust Fuzz Book](https://rust-fuzz.github.io/book/cargo-fuzz/setup.html) describes
the toolchain requirements; [LLVM's libFuzzer documentation](https://llvm.org/docs/LibFuzzer.html)
describes corpus feedback and engine flags.

Both drivers retain source, binary and initial fixture hashes, exact commands,
build logs, stdout/stderr, status and environment. The sanitizer driver additionally
records the final corpus manifest, failure artifact hashes and raw engine stats.
Repeated runs require new stems and never overwrite earlier evidence.

## Evidence and limits

Two checked deterministic campaigns pass: 2,000,000 mutations plus 162 seed
replays on x86_64 Windows, and the same counts with a different PRNG seed on i686
Windows. Every tracked parser family is successfully entered in both runs.
The 32-bit run exercises narrower address arithmetic; virtual >4 GiB tests remain
separate. This is bounded mutation evidence and does not establish exhaustive
malformed-input safety.

The Windows x86_64 nightly/ASan campaign also passes: 2,000,000 executions in
660.14 seconds, 4,194 added corpus units, and no panic/sanitizer failure artifact.
Raw logs, complete initial/final corpus hashes, binary/source/runtime fingerprints
and commands are retained in `validation/fuzz-asan-2m.*`. The final corpus directory
has 1,474 files; the engine's selected corpus is 1,301 units. Its final counters
are 3,538 edges and 8,056 features. Added, selected and physical corpus counts
differ as inputs are reduced/replaced and original seed files remain on disk.
The two checked portable reports are `validation/fuzz-mutation-{x64,i686}-2m.json`.
All 42 integration tests pass on x86_64; the new mutation test also passes in
release and on i686, alongside the earlier 41-test release/i686 checks.

After the measured metadata word-scan optimization, all 44 tests pass in x64
debug/release and i686 debug, as does Clippy. The checked deterministic campaigns
were repeated: four million mutations plus 324 seed replays pass. Each bounded
nonempty input also compares the NUL search against a scalar oracle. The complete 1,474-file
earlier libFuzzer corpus replays successfully and matches its original hashes.
A fresh ASan campaign with a different seed passes 500,000 executions in 184.64
seconds, adds 2,867 units, and produces no failure artifact. These final-source
reports are `validation/fuzz-word-*.json`; raw ASan logs are retained alongside them.
Metadata field truth remains exact for all 12 independent external fixtures, with
the same nine exact dr_wav comparisons and three documented trailing-RF64 gaps.

Both complete coverage-generated corpora are preserved as deterministic ZIPs in
`validation/fuzz-asan-2m-corpus.zip` and `fuzz-word-asan-500k-corpus.zip`. Sidecars
record archive/report hashes and byte counts. `tests/archive_fuzz_corpus.py`
verifies every file against the completed run manifest before archiving, then
verifies the archive contents again. The archives contain generated fixture and
mutated bytes only; no external codec source is embedded. Replay without depending
on the original target directory:

```sh
python -m zipfile -e crates/deadlib-audio/decode/wav/validation/fuzz-asan-2m-corpus.zip target/wav/replay-corpus
python crates/deadlib-audio/decode/wav/tests/run_mutation.py --iterations 0 --corpus target/wav/replay-corpus --output target/wav/replay-result
```

Use fresh extraction/output directories. The optional `--corpus` mode copies and
hashes supplied seeds into each new run; its report identifies those seeds as an
existing corpus rather than independent fixture generation.

Engine edge/feature counts are not source line-coverage percentages.
Sanitizer process memory includes instrumentation/runtime overhead and must not be
reported as the normal codec's peak RSS. Longer corpora, more architectures,
large-file/resource limits beyond this harness, injected arbitrary I/O failures,
and unsupported format variants still require validation.
