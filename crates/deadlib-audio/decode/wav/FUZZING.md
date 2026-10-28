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

The shared harness now includes known-length output parity and forward native/
ADPCM state checks. Forward full acceptance implies borrowed container acceptance;
valid format-before-data inputs additionally compare raw native frames and exact
sequential ADPCM samples/errors. Two one-million-mutation runs on x64/i686 pass,
as do all 1474 earlier corpus inputs. The fresh 100000-execution ASan run passes
in 66.83 seconds with 1757 added units and no failure artifact. Reports are
`validation/fuzz-forward-*`; its complete 1108-file corpus is preserved in
`fuzz-forward-asan-100k-corpus.zip`. These checks also cover the known-length muxer
and writers introduced immediately before the forward readers. All 56 crate
tests and Clippy pass as described in STREAMING.md.

Three-bit WAV IMA adds 60 independently packed seeds (222 total): four container
spellings, both header byte orders, 1/2/3/5 channels, interleaved bit/word crossings,
short blocks and valid encoder geometries. The shared block/reader/writer properties
now exercise 3-bit decoding and encoding too. One million checked mutations each
on x64/i686 pass, as do all 1474 earlier corpus files. The fresh 100000-execution
ASan run passes in 64.98 seconds, adds 1938 units and leaves no failure artifact.
Reports and raw logs are `validation/ima3-{mutation-x64,mutation-i686,prior-replay,asan}.*`;
the complete 1211-file corpus is retained in `ima3-asan-corpus.zip` with a verified
archive/report sidecar. All report source inputs remained unchanged during runs.
The ASan peak of 261 MiB includes instrumentation and runtime overhead, and does
not measure normal codec RSS. ADPCM.md records independent sample truth and the
four-reference comparison; all 60 tests and Clippy pass.

Native wavl/slnt adds 96 independently packed seeds (318 total), a seventeenth
reported family, segment/count/metadata boundaries and parity between incremental
and known-length wavl writers. Native range comparisons now read physical frames
through the view instead of assuming a contiguous byte slice. Forward comparison
uses a bounded decoded prefix and final validation, so huge virtual silence in a
small mutated input cannot cause unbounded PCM expansion in the harness.

`validation/wavl-mutation-{x64,i686}.*` records one million checked mutations per
architecture. Both pass, including over 191000 successful wavl-family entries in
each run. `wavl-asan.*` records 100000 fresh executions in 150.87 seconds, 1902
added units and no failure artifact. The complete 1413-file generated corpus is
preserved in `wavl-asan-corpus.zip` with verified hashes. Its 260 MiB ASan peak
includes instrumentation/runtime overhead and is not normal codec RSS.

The earlier two-million-run and three-bit-IMA corpus archives are verified and
combined by content hash: all 2507 unique inputs replay successfully with the
new properties. `wavl-prior-corpus-provenance.json` records both archive hashes;
`wavl-prior-replay.*` records every replayed input hash and result. All listed
source inputs remained unchanged during these campaigns. The full 69-test suite,
Clippy, the 248-case wavl oracle, 460-case flat native regression and 204-case
earlier ADPCM range regression also pass. WAVL.md records exact API/format scope.

The compact PCM view repeats the 318-seed campaign without changing the shared
properties: `validation/compact-mutation-{x64,i686}.*` each pass one million
checked mutations, and `compact-prior-replay.*` passes all 2507 earlier unique
inputs. `compact-asan.*` passes 100000 executions in 121.34 seconds, adds 2010
units and produces no failure artifact. Its complete 1443-file corpus is retained
in `compact-asan-corpus.zip` with the verified archive/report sidecar. The 270 MiB
ASan peak includes instrumentation/runtime overhead; it is not normal codec RSS.
All campaign source hashes remain unchanged. The 708-case native replay retains
4484 descriptor/decode/range hashes and 1912 exact writer outputs, and all 69
tests pass in x64 debug/release and i686 debug along with Clippy. PERFORMANCE.md
records the fixed-state size reduction and the full-format-query time tradeoff.

The RF64 duration correction adds 40 independently packed seeds (358 total),
crossing ordinary/sentinel fact, leading/trailing placement, native wavl and flat
IMA, both RF64/BW64 spellings and unused zero/small/u64::MAX ds64 fields. The
shared native-forward property now checks that any reported duration equals the
borrowed sequence length; a duration may remain unavailable until its boundary.
Explicit forward ADPCM parity exercises the independent unused-field cases too.

`validation/rf64-count-mutation-{x64,i686}.*` each pass one million checked
mutations. `rf64-count-prior-replay.*` passes all 1443 inputs from the immediately
preceding archived ASan corpus. `rf64-count-asan.*` passes 100000 fresh executions
in 137.16 seconds, adds 1915 units and produces no failure artifact. The complete
generated corpus is retained in `rf64-count-asan-corpus.zip` with a verified
archive/report sidecar. Its 259 MiB instrumented peak is not normal codec RSS.
All campaign source inputs remain unchanged. All 71 tests pass on x64
debug/release and i686 debug; Clippy, all 708 retained native gates and all 432
retained forward ADPCM outputs also pass. STREAMING.md records the duration API
contract and distinguishes RF64 rules from BW64 compatibility extensions.

The BW64 correction adds 18 independent dummy-field seeds (376 total), covering
PCM/IMA, absent/ordinary/0xFFFFFFFF fact and raw zero/one/u64::MAX dummy words.
Existing BW64 sentinel seeds now cover rejection of an RF64-only extension.
Generated native flat/wavl output is checked for zero dummy words in addition
to sample and count parity. Reader properties retain raw input fields unchanged.

`validation/bw64-dummy-mutation-{x64,i686}.*` each pass one million checked
mutations. All 1414 inputs from the preceding archived corpus replay successfully
(`bw64-dummy-prior-replay.*`). `bw64-dummy-asan.*` passes 100000 executions in
113.17 seconds, adds 1936 units and produces no failure artifact. Its complete
1490-file corpus is retained in `bw64-dummy-asan-corpus.zip` with a verified
archive/report sidecar. The 261 MiB instrumented peak is not normal codec RSS.
All recorded source inputs remain unchanged. All 74 tests pass on x64
debug/release and i686 debug, with Clippy, the native replay, 432 forward ADPCM
outputs and 24 physical sparse-file probes. BW64.md records the changed count
policy and the precise eight-byte writer-output comparison rule.

Extensible IMA adds 96 independent seeds (472 total): file and block selectors,
3/4-bit word geometries, RIFF/RIFX/RF64/BW64, 1/2/6/33 channels, masks and both fact
placements. Generated IMA output now checks that the force_extensible option
selects the extensible tag and the correct samples-per-block word. The unresolved
forced Microsoft mapping is explicitly checked for Unsupported.

`validation/ima-extensible-mutation-{x64,i686}.*` each pass one million checked
mutations. All 1490 prior archived inputs replay successfully
(`ima-extensible-prior-replay.*`). `ima-extensible-asan.*` passes 100000 executions
in 113.55 seconds, adds 1891 units and produces no failure artifact. Its complete
1580-file corpus is preserved in `ima-extensible-asan-corpus.zip` with a verified
archive/report sidecar. The 263 MiB instrumented peak is not normal codec RSS.
All listed campaign inputs remain unchanged. The full 78-test suite, Clippy,
416 independent fixture/encoder gates, 432 earlier forward outputs and 24 updated
physical sparse-file probes pass. IMA_EXTENSIBLE.md distinguishes primary-field
interpretation, direct reader parity, paired legacy evidence and unresolved scope.

Generic segmented output adds a separately reported eighteenth family, wavl_mux,
and 12 opaque/vendor-GUID/native descriptor seeds (484 total). The block selector
now also sends any valid bounded raw descriptor through known-length and patched
muxers, checking complete byte equality, raw fmt preservation, aggregate counts,
overfill rejection, container/seekable/forward acceptance and child inventory.
Native data counts derive from alignment; encoded counts remain producer supplied.
Both byte orders and RIFF/RF64/BW64 flags are exercised without codec dispatch.

`validation/wavl-mux-mutation-{x64,i686}.*` each pass one million checked mutations,
including over 145000 successful entries into the new family per architecture.
All 1580 prior corpus inputs pass (`wavl-mux-prior-replay.*`). `wavl-mux-asan.*`
passes 100000 fresh executions in 131.19 seconds, adds 1901 units and produces no
failure artifact. Its complete 1581-file corpus is preserved in
`wavl-mux-asan-corpus.zip` with a verified archive/report sidecar. The 260 MiB
instrumented peak is not normal codec RSS. All listed inputs remain unchanged.
The full 83-test suite, Clippy and the 708-case native byte/sample replay pass.
WAVL.md documents the producer boundary; PERFORMANCE.md retains the native
output regression and does not claim competitive performance from these checks.

Borrowed compressed sequences add a nineteenth reported family, adpcm_wavl,
and 216 independent seeds (700 total). They cover Microsoft, both IMA widths,
legacy/extensible IMA, 1/2/6/33 channels where applicable, all containers, initial
and held silence, empty data, complete short blocks, metadata and fact trimming.
The shared harness constructs AdpcmWavl independently of the flat wrappers and
compares bounded prefixes/ranges with a sequential child/block model that does
not use its index or data_frames helper. Expansion is capped at 1024 samples;
scratch remains capped at 8192 samples, blocks at 4096 bytes, channels at 256,
input at 32768 bytes and child traversal at the existing 64-chunk limit. Invalid
block-header acceptance and requested empty/partial ranges are checked too.
Both campaign runners now fingerprint imported fixture generators as well.

`validation/adpcm-wavl-mutation-{x64,i686}.*` each pass one million checked
mutations, including 102831/102676 successful new-family entries. All 1581 inputs
from the prior archived corpus pass (`adpcm-wavl-prior-replay.*`); those older
inputs do not enter the new family. `adpcm-wavl-asan.*` passes 100000 fresh
executions in 121.30 seconds, adds 1899 units and produces no failure artifact.
The final 1827-file corpus is retained in `adpcm-wavl-asan-corpus.zip`, SHA-256
`007e6b967715c609e9371360c66827f38d7356b7a7fc969e1b5c6d89539b4085`,
with a verified archive/report sidecar. Its 260 MiB instrumented peak is not
normal codec RSS. All listed campaign inputs remain unchanged. The complete
87-test suite and Clippy pass on the recorded configurations; 216 independent
fixtures/1296 borrowed ranges and all 708 retained native gates also pass.
WAVL.md records the count/padding and file/forward limitations.

The owning segmented reader adds a twentieth family, adpcm_wavl_reader, using
the same 700 seeds. Before decoding, the harness compares bounded borrowed/file
descriptor and fact acceptance. Its sequential prefix model now checks both
implementations under short/Interrupted I/O, then compares exact seeks, EOF,
partial caller frames and child inventory. The owned wrapper reuses the original
inventory; invalid block headers and explicit seek recovery share the existing
failure contract. Input, scratch, sample-expansion and child-count limits remain
those recorded above. The new properties do not cover forward segmented ADPCM.

`validation/adpcm-wavl-reader-mutation-{x64,i686}.*` each pass one million checked
mutations, with 102370/102958 successful owning-reader family entries. All 1827
previous archived inputs pass (`adpcm-wavl-reader-prior-replay.*`), including 244
successful entries into both segmented ADPCM families. `adpcm-wavl-reader-asan.*`
passes 100000 executions in 135.19 seconds, adds 1861 units and produces no
failure artifact. Its final 1857-file corpus is retained in
`adpcm-wavl-reader-asan-corpus.zip`, SHA-256
`9ee28fce216e810eea4601b0f054d3046c10de349fed618006746fc43fdffc7a`,
with the verified archive/report sidecar. The 261 MiB instrumented peak is not
normal codec RSS. Listed campaign inputs remain unchanged. All 89 tests, Clippy,
216 independent fixtures/2592 borrowed-file-range checks, 204 prior real-file
ADPCM ranges and the 708-case native byte/sample replay pass. WAVL.md and
PERFORMANCE.md retain the error/cache/memory contract and measured regressions.

Forward segmented ADPCM adds a twenty-first family, adpcm_wavl_forward, with the
same 700 independent seeds. The bounded sequential prefix model now checks all
three implementations. The Read-only adapter uses short/Interrupted I/O, matches
requested sample bits/error acceptance and finishes the unread suffix to check
geometry, fact and child boundaries. Prefix expansion remains capped at 1024
samples, with the existing 8192-sample scratch/4096-byte block/256-channel limits.
Forward constructor acceptance alone does not certify unseen child geometry;
deferred failures and intentionally unread block-header behavior have direct tests.

`validation/adpcm-wavl-forward-mutation-{x64,i686}.*` each pass one million checked
mutations, with 102562/102648 successful forward-family entries. All 1857 earlier
archived corpus inputs pass (`adpcm-wavl-forward-prior-replay.*`), including 243
entries into the three segmented ADPCM families. `adpcm-wavl-forward-asan.*`
passes 100000 executions in 150.48 seconds, adds 1908 units and produces no
failure artifact. Its final 1839-file corpus is retained in
`adpcm-wavl-forward-asan-corpus.zip`, SHA-256
`01f9c01f6e6e783438465bc449a6fdef071cc6b43dd8a5053c169b7cc1878d85`,
with the verified archive/report sidecar. The 264 MiB instrumented peak is not
normal codec RSS. All listed campaign inputs remain unchanged. All 92 tests,
Clippy, 216 independent fixture gates/3024 three-API checks, 432 earlier real-file
forward outputs and the 708-case native byte/sample replay pass. WAVL.md records
the known-duration and finalization contract; PERFORMANCE.md records state growth,
unchanged block-buffer allocation and narrow timing scope.
