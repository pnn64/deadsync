# Revision comparison harness

The dated [2026-11-28 report](../results/windows-x86_64-20261128/README.md) records the source revisions, input hashes, build settings and paired measurements. This harness compares two Rust revisions rather than comparing to FFmpeg's decoder speed. It uses FFmpeg only for input preparation and independent output verification.

Run on Windows x64 with Rust, Python 3.11+ and FFmpeg on the path. Use a disposable checkout, with the working directory at the repository root. Start at the report commit:

```powershell
& crates/deadlib-video-h264/bench/revision-pass/prepare.ps1
```

In that disposable checkout, switch to each revision listed in the report and run the following command with the corresponding stage name (stage0, stage1, stage2 or stage3). The prepared harness and input files remain under the ignored target directory.

```powershell
& target/h264-perf-20261128/build.ps1 stage0
```

After building the four stages, run the paired comparisons and correctness checks. The fixed affinity in these scripts is logical CPU 42; choose an available idle logical CPU consistently if reproducing on another host.

```powershell
py target/h264-perf-20261128/measure.py stage0 stage1 7 srpg9,srpg10,cavlc parse-annex,surface-annex,packed-annex
py target/h264-perf-20261128/measure.py stage1 stage2 7 srpg9,srpg10,cavlc parse-avcc,surface-avcc,packed-avcc
py target/h264-perf-20261128/measure.py stage1 stage2 11 srpg9 surface-avcc,surface-annex
py target/h264-perf-20261128/measure.py stage2 stage3 7 srpg9,srpg10,cavlc peek,surface-avcc,packed-avcc
py target/h264-perf-20261128/measure.py stage0 stage3 7 srpg9,srpg10,cavlc parse-annex,parse-avcc,surface-avcc,packed-annex
py target/h264-perf-20261128/verify.py stage0 stage1 srpg9,srpg10,cavlc surface-annex,packed-annex
py target/h264-perf-20261128/verify.py stage1 stage2 srpg9,srpg10,cavlc surface-avcc,packed-avcc
py target/h264-perf-20261128/verify.py stage2 stage3 srpg9,srpg10,cavlc surface-avcc,packed-avcc
py target/h264-perf-20261128/verify.py stage0 stage3 srpg9,srpg10,cavlc surface-avcc,packed-annex
```

The CSV files are in the target directory. Pass those paths to summary.py for the paired medians and ranges. The verification writes and deletes a temporary raw frame file, comparing its SHA-256 with FFmpeg's visible-plane hash.
