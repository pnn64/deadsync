# Entropy decoding revision comparison

The [2026-11-28 report](../results/windows-x86_64-20261128-entropy/README.md) contains the source revisions, input hashes, paired cycle measurements and heap traffic. The harness uses separate executables for the isolated replay and full decoder.

Run on Windows x64 with Rust, Python 3.11+ and FFmpeg. Use a disposable checkout of the repository, with the working directory at its root. Start at the report commit:

```powershell
& crates/deadlib-video-h264/bench/entropy-pass/prepare.ps1 -Destination C:\GitHub\h264-perf-external-20261128
```

This downloads five independent H.264 media files into the external directory, extracts native excerpts, creates two external-footage CAVLC transcodes, and recreates the original game inputs. It also prepares length-prefixed AVCC inputs. It rejects a destination inside the repository. No downloaded media, extracted samples, or sampled RBSP bytes belong in Git.

In the disposable checkout, switch to the baseline revision listed in the report. Build the baseline and capture a sampled trace from the instrumented copy of that revision (the checkout itself is not instrumented):

```powershell
$d = 'C:\GitHub\h264-perf-external-20261128'
$crate = (Resolve-Path crates/deadlib-video-h264).Path
& "$d/build.ps1" pass0 "$crate/src"
& "$d/trace.ps1" $crate
```

Switch to each subsequent accepted revision and invoke the same build command with pass1, pass2 or pass3. The trace files must remain the baseline traces for all stages.

Run the comparisons with seven rotating pairs and two independent allocation runs:

```powershell
$targeted = 'cavlc,bunny-cavlc,nature-cavlc,africa-excerpt'
$all = 'cavlc,bunny-cavlc,nature-cavlc,africa-excerpt,srpg9,srpg10,bunny-excerpt,nature-excerpt,envivio-excerpt,interlaced-excerpt'
py "$d/measure.py" pass0 pass1 7 $targeted residual
py "$d/measure.py" pass0 pass1 11 srpg9,srpg10 surface-annex decodeonly42
py "$d/measure.py" pass0 pass1 7 cavlc,bunny-cavlc,nature-cavlc,africa-excerpt,bunny-excerpt,nature-excerpt,envivio-excerpt,interlaced-excerpt surface-annex decodeonly42
py "$d/measure.py" pass0 pass1 7 $targeted packed-avcc decodeonly42
py "$d/measure.py" pass1 pass2 7 $targeted ue
py "$d/measure.py" pass1 pass2 7 $all surface-annex
py "$d/measure.py" pass1 pass2 7 $targeted packed-avcc
py "$d/measure.py" pass1 pass2 11 cavlc packed-avcc repeat42
py "$d/measure.py" pass2 pass3 7 $targeted residual
py "$d/measure.py" pass2 pass3 7 $all surface-annex
py "$d/measure.py" pass2 pass3 7 $targeted packed-avcc
$env:H264_REPEATS = '1'
py "$d/measure.py" pass2 pass3 11 $targeted residual cold
Remove-Item Env:H264_REPEATS
py "$d/measure.py" pass2 pass3 11 interlaced-excerpt,bunny-excerpt surface-annex repeat42
py "$d/measure.py" pass0 pass3 7 $all surface-annex
py "$d/measure.py" pass0 pass3 7 $targeted packed-avcc
py "$d/measure.py" pass0 pass3 7 $targeted residual,ue
py "$d/measure.py" pass0 pass3 11 bunny-cavlc,interlaced-excerpt surface-annex repeat42
```

Use the optional label and logical CPU arguments of measure.py for additional independent batches. The default logical CPU is 42; choose an available idle CPU consistently on other hosts. The initial mixed-harness pass0/pass1 full-decode comparisons can be reproduced by setting H264_MIXED_BENCH=1; they are retained as exploratory evidence. The rejected fixed-width read source was reverted, so that prototype's raw samples are historical evidence rather than a supported build.

For correctness, each adjacent pair and the final baseline/final pair can be checked:

```powershell
py "$d/verify.py" pass0 pass3 $all surface-annex
py "$d/verify.py" pass0 pass3 $targeted packed-avcc
```

The verifier writes a temporary YUV output outside the checkout and compares its SHA-256 with the independent FFmpeg visible-plane hash in display order. Raw paired CSVs and input identities are the reproducible evidence; the source media and traces stay external.
