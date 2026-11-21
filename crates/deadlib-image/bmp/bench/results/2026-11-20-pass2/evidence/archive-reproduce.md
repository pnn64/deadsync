# Reproducing BMP decoder pass 2

The retained production code is the original implementation at
`945da378a907bb7c4fec1f432d02d320c9d2a462`. The only retained code addition is
the public regression coverage in `evidence/retained-regression-tests.patch`.
All performance candidates in this archive were rejected. Their patches are
preserved for reproducing the experiments, not as approved production changes.

Use a fresh checkout/worktree at that revision and preserve unrelated work.
Match the host/compiler/target/configuration recorded in
[evidence/baseline/identity.json](evidence/baseline/identity.json).
The benchmark is a standalone workspace and pins image 0.25.10 in its own
Cargo.lock. A full application build is unnecessary.

## Build and freeze the original baseline first

From the repository root, use a new task directory:

```powershell
cargo build --manifest-path crates/deadlib-image/bmp/bench/Cargo.toml --release --locked --offline --bins -j 2 --target-dir target/bmp-pass2-reproduction/timing
cargo build --manifest-path crates/deadlib-image/bmp/bench/Cargo.toml --release --locked --offline --features allocations --bin bmp-codec-bench -j 2 --target-dir target/bmp-pass2-reproduction/allocations
```

The original initial build used `-j 1`; later builds used `-j 2`. Compilation
was outside all measurement leases. `--offline` assumes the pinned dependencies
are already cached; acquire them separately if needed.

Copy the timing, allocation, and snapshot executables to new baseline-specific
filenames before changing source or rebuilding a shared cache. Preserve their
hashes. The original names/hashes are in
[evidence/baseline/executables.json](evidence/baseline/executables.json).
Executable bytes also depend on toolchain and build paths; source/configuration
hashes identify the reproduction inputs.

The release profile is opt-level 3, LTO, one codegen unit, panic abort, debug 1,
with repository target flags and static CRT configuration. Do not change these
between parent and candidate.

## Exact native behavior and independent tests

Capture a snapshot with the frozen original `native_snapshot.exe capture PATH`
command, where PATH does not exist. It must contain 22,570 records and 42,819,895
bytes with SHA-256
`076e78cae341e782b60f72fa3f60c0ff85142759e4821069f1012063f37bf0df`.
Then use each rebuilt candidate's `native_snapshot.exe verify PATH` command.
The helper compares complete serialization, including inputs, output samples,
metadata, limits, error variants/messages, and encoder bytes.

The original snapshot payload is intentionally omitted from this compact archive.
Its generator is pinned in the repository. Candidate 5 matched it exactly; after
production was restored byte for byte, the original production/harness identity
made an additional snapshot or benchmark rebuild unnecessary.

For the retained test-only change, apply
`evidence/retained-regression-tests.patch` to the pinned baseline. The standalone
validation package in `evidence/validation` references the actual BMP source
and integration tests with relative paths. Copy it to
`target/NEW-TASK/validation`, and place regenerated alpha controls beside it at
`target/NEW-TASK/alpha-controls`. Run its tests and strict all-target Clippy with
a separate target directory. The final exact commands and native exits are in
[evidence/final-restored-validation/commands.jsonl](evidence/final-restored-validation/commands.jsonl).

Capture native stdout and stderr directly with Python
`subprocess.run(..., stdout=file, stderr=subprocess.STDOUT)` and check the native
exit status. Early PowerShell transcripts omitted Cargo's native output;
candidate 4, candidate 5, and final restored validation use direct capture.
The complete status is in [validation-status.json](validation-status.json).

## Applying experimental source

Each `evidence/STAGE/changes.patch` is cumulative from the pinned original
baseline. Apply it only to that baseline. `parent-delta.patch` applies instead
to the named `parent_stage` in that stage's identity. Do not apply both.

Stages are `candidate1-alpha`, `candidate1-logical`,
`candidate2-rgba4444`, `candidate3-direct-rgba`,
`candidate4-alpha-dispatch`, and `candidate5-two-layouts`.
Candidate 5 is rebased directly on the original baseline, not on candidate 4.
Its exact two-section source proof is retained. The final source restoration
is documented in
[evidence/restored-baseline-source.json](evidence/restored-baseline-source.json).

Build and freeze each experimental stage independently. Run the snapshot,
independent tests, and separate allocation measurements before timing. Never
overwrite a frozen executable or a previous result. A failed parent must not
remain silently in the lineage of a claimed retained descendant.

## Full timing and allocation matrix

Set these variables, keep `BMP_BENCH_SIZES` absent, and run the frozen timing
program separately in each mode:

```powershell
$env:BMP_BENCH_CPU = '2'
$env:BMP_BENCH_MS = '20'
$env:BMP_BENCH_SAMPLES = '5'
target/bmp-pass2-reproduction/baseline/bmp-timing.exe --mode decode
target/bmp-pass2-reproduction/baseline/bmp-timing.exe --mode owned-rgba
target/bmp-pass2-reproduction/baseline/bmp-timing.exe --mode rgba
```

These commands assume the preserved executables were copied to the new
baseline directory above. The row counts are 120, 218, and 218. Each operation
includes owned output destruction; generation and file I/O are outside timing.
Image reference timing is included only for exact RGBA matches. Keep every
`NON_EQUIVALENT` diagnostic and all native-only comparisons.

The repository corpus command uses consuming mode plus these positional paths:

```text
tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files
tests/fixtures/itgmania-primary-bitmaps
tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files/misnamed.png
```

It emits 24 rows for twelve valid files and explicitly reports the intentionally
invalid unused-entry file. The extra positional file includes the BMP with a PNG
extension. All thirteen repository input identities are pinned in the baseline
manifest.

Use identical arguments with the separately instrumented allocation executable.
It compiles out timing and emits zero timing fields. There are 622 paired rows
across decode, consuming, borrowed, repository, and the alpha controls. They
include references and do not mean 622 distinct native inputs.

The archived `measure.ps1` captures stdout as TSV, stderr as a log, and settings,
executable hash, timestamps, and native exit status as JSON. Its task path is the
original `target/bmp-decode-opt-20261120-pass2`; adapt only paths for a fresh
reproduction. Commands use exclusive output creation and must not be replayed
against the original completed task directory.

## Alpha controls and focused experiments

Copy `evidence/generate-alpha-controls.py` into a new task directory and run it
there. It creates `alpha-controls` exclusively: 18 BMPs totaling 52,922,892
bytes at 17x9, 511x257, and 1920x1080, both orientations, with zero or one nonzero
alpha at the first/last logical pixel. Compare the generated manifest with
[evidence/alpha-controls/manifest.json](evidence/alpha-controls/manifest.json),
which includes source and independently generated expected-RGBA hashes.

Do not precreate or overwrite its output directory. The generated BMP payloads
are omitted because they are large and exactly reproducible. The task-local
alpha test checks every output pixel independently.

Focus command arrays preserve the original filters, fixture order, five-batch
durations, process order, and executable choice. The final candidate 5 full
matrix uses baseline/candidate order for decode and borrowed, candidate/baseline
for consuming and repository. Alpha controls use 80 ms batches, decode AB and
consuming BA. Earlier isolated single-file repetitions preserve every ABBA/BAAB
median; none selects the fastest run.

## Measurement isolation and interpretation

Run no compiler, tests, other codec benchmark, or other heavy work during a
measurement lease. The final command files use the strict watcher and fresh
JSONL logs. The older watcher and stricter watcher have different recorded
hashes; do not claim strict coverage for earlier runs.

The final strict watcher is
`evidence/watch_commands-strict.py`, SHA-256
`ea1401c8a20aa050359d31ef88e59b04a22ed3b3827c29d3ca9295c3d108c471`.
Pass a command-array JSON file and a new `--log` path. It samples relevant
external processes every two seconds, preserves command outcomes, and stops
on detected contamination. It cannot prove absence of all background activity.

All ratios in the final report and candidate 3–5 summaries are after/before.
Some earlier generic comparison reports use the inverse; their headings say
which convention applies. Five-batch min/max ranges are descriptive, not
confidence intervals. Report unchanged reference controls, every loss, policy
differences, and process-context effects. Never use the target geometric mean
as a whole-matrix or application-speed claim.

## Archive contents and limits

The archive contains small raw results, logs, command arrays, hashes, cumulative
and parent patches, independent-control manifests, and final validation.
It excludes executables, PDBs, build caches, generated BMP payloads, the large
snapshot, and full disassembly. Candidate 2's symbol summary retains the
specific evidence that `masked_row` was inlined; that does not establish a
cause for the measured regressions.

[manifest.json](manifest.json) records the hash and size of every archived file
except itself. Historical capture-time identities and pre-run plans can still
say validation/timing was pending; the later completed result sidecars,
preparation summaries, and final ledger supersede those statuses without
rewriting historical evidence.
