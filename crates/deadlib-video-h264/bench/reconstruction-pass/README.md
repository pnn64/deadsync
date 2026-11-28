# Reconstruction revision comparison

The [third 2026-11-28 report](../results/windows-x86_64-20261128-reconstruction/README.md) records independent-input CABAC screening and reconstruction comparisons. It includes paired cycles, cycle-normalized throughput, heap traffic, input identities and rejected trials.

Run on Windows x64 with Rust, Python 3.11+ and FFmpeg. Use a disposable checkout. Start with the preceding [entropy pass](../entropy-pass/README.md) to prepare the external media, then from the report revision run:

```powershell
& crates/deadlib-video-h264/bench/reconstruction-pass/prepare.ps1 -Previous C:\GitHub\h264-perf-external-20261128 -Destination C:\GitHub\h264-perf-cabac-20261128
$d = 'C:\GitHub\h264-perf-cabac-20261128'
$src = (Resolve-Path crates/deadlib-video-h264/src).Path
$all = 'srpg9,srpg10,bunny-excerpt,nature-excerpt,envivio-excerpt,interlaced-excerpt,cavlc,bunny-cavlc,nature-cavlc,africa-excerpt,bunny-custom'
$original = $all.Replace(',bunny-custom','')
$eight = 'srpg9,srpg10,bunny-excerpt'
$packed = 'srpg9,srpg10,bunny-cavlc,africa-excerpt,bunny-custom'
```

In the disposable checkout, switch to the baseline revision listed in the report. Build the baseline and capture traces from instrumented copies, not the checkout:

```powershell
& "$d/build.ps1" cab0fresh -Source $src
& "$d/trace.ps1" -Kind cabac -Source $src -Cases 'srpg9,srpg10,bunny-excerpt,nature-excerpt,envivio-excerpt,interlaced-excerpt'
& "$d/trace.ps1" -Kind transform -Source $src -Cases $all
```

Switch to each subsequent accepted revision and build with the corresponding stage name from the report. For chroma replay, capture the chroma trace at cab2, before building the subsequent chroma candidate:

```powershell
& "$d/build.ps1" cab2 -Source $src
& "$d/trace.ps1" -Kind chroma -Source $src -Cases $all
```

Retain the same traces across both stages of each comparison. The extraction helper copies the actual chroma transform fragment from each revision for isolated replay. The full decoder executable has no trace or replay instrumentation. Each isolated batch replays the sampled operations 256 times by default; allocation measurement is separate and applies to full decoding. Cycle-normalized throughput counts operations or decoded frames per billion cycles.

```powershell
py "$d/measure.py" cab0fresh cab1 9 $eight dq8
py "$d/measure.py" cab0fresh cab1 9 bunny-custom dq8 custom
py "$d/measure.py" cab0fresh cab1 7 $original surface-annex
py "$d/measure.py" cab0fresh cab1 7 $eight packed-avcc
py "$d/measure.py" cab0fresh cab1 9 bunny-custom surface-annex,packed-avcc
py "$d/measure.py" cab0fresh cab1 11 srpg9 surface-annex repeat42
py "$d/measure.py" cab1 cab2 7 $all dq4
py "$d/measure.py" cab1 cab2 7 $all surface-annex
py "$d/measure.py" cab1 cab2 7 $packed packed-avcc
py "$d/measure.py" cab2 cab3 7 $all chroma4
py "$d/measure.py" cab2 cab3 7 $all surface-annex
py "$d/measure.py" cab2 cab3 7 $packed packed-avcc
py "$d/measure.py" cab2 cab3 11 envivio-excerpt,bunny-excerpt,bunny-cavlc,cavlc,bunny-custom surface-annex repeat42
py "$d/measure.py" cab2 cab3 11 bunny-excerpt,bunny-cavlc,cavlc surface-annex repeat2 2
foreach ($k in 'cycles','alloc') {
    Copy-Item "$d/cab2-decode-$k.exe" "$d/cab2control-decode-$k.exe"
}
py "$d/measure.py" cab2 cab2control 11 bunny-excerpt surface-annex aa2 2
py "$d/measure.py" cab0fresh cab3 7 $all surface-annex
py "$d/measure.py" cab0fresh cab3 7 $all packed-avcc
py "$d/measure.py" cab0fresh cab3 11 srpg10,nature-cavlc,africa-excerpt surface-annex repeat2 2
py "$d/measure.py" cab0fresh cab3 11 srpg9 packed-avcc repeat2 2
```

An optional label and logical CPU argument identify separate paired batches; the default CPU is 42. The earlier cab0 executable reused the previous pass's final decoder; cab0fresh was independently rebuilt to check baseline variation. The reverted trial sources are historical evidence rather than supported builds. The raw CSVs preserve their measured results.

Use the verifier for adjacent revisions and the final baseline/final comparison. It checks full YUV bytes against independent FFmpeg hashes.

```powershell
py "$d/verify.py" cab0fresh cab3 $all surface-annex
py "$d/verify.py" cab0fresh cab3 $all packed-avcc
```

All downloaded media, extracted samples, raw traces, temporary YUV output and executables remain outside Git. Only source, the measurement harness and numerical evidence are versioned.
