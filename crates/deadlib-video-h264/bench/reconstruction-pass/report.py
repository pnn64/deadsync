import csv, hashlib, pathlib, statistics as stats, sys
from collections import defaultdict

root = pathlib.Path(__file__).resolve().parent
out = pathlib.Path(sys.argv[1])
revisions = sys.argv[2:]
def sha(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()

lines = [
    "# H.264 CABAC and reconstruction revision comparison",
    "",
    "Windows x64, Xeon E5-2696 v4, Rust 1.100.0; pinned logical CPU 42 unless otherwise specified. The direct release build uses opt-level=3, 16 decoder codegen units, default CPU, std and surface, no LTO. Full-decode timings use QueryThreadCycleTime and separate allocation-meter builds. The dedicated decoder executables have no trace or replay code. The external sample corpus and hashes are inherited from the preceding entropy pass; no media or captured coefficient data is committed.",
    "",
    "The isolated harness replays every 512th CABAC arithmetic decision, every 128th CABAC residual block, all slice initialization arguments, and every 64th reconstruction operation, using captured input state from actual video decoding. Chroma replay uses the actual reconstruction fragment extracted from each source revision and every 64th non-skipped chroma block recorded before the transform, including the decoder's existing has_ac flag. Replay includes operation setup and result consumption. Isolated replay is timed without allocation instrumentation; the functions under test do not allocate. Full-decode allocation calls include alloc, alloc_zeroed and realloc; requested bytes count traffic rather than peak memory.",
    "",
    "Cycle and throughput figures are individual medians; paired percentage changes use medians of before/after ratios. The absolute cycle totals below cover the entire measured batch (256 trace repetitions by default, 4096 for initialization, one stream for full decoding); dividing by repeats gives cycles per trace replay. Throughput is operations or frames per billion CPU cycles. Negative cycle and allocation changes are improvements. Ranges include all samples, including outliers. Small changes with substantial variation are inconclusive. The full pipeline includes framing, RBSP extraction, parameter sets, decoding, output ordering, consumption, draining and destruction. The surface path uses SPS reorder depth; packed output uses the default depth.",
    "",
    "Source revisions: " + "; ".join(f"cab{i} = `{rev}`" for i, rev in enumerate(revisions)) + ". cab0fresh is a fresh direct build of the same H.264 source as cab0, which reused the preceding pass's baseline executable.",
    "",
    "The trialnorm, trialmask and trialselect experiments were screened with actual residual/bin replay and rejected before full decoding. trialseq/trialseqfull specialized consecutive CABAC significance contexts; it had modest isolated gains but reproduced a roughly 4% SRPG10 surface regression and was reverted. trialinit/trialinitfull narrowed initialization arithmetic to i16 and substantially accelerated isolated initialization, but repeatedly increased SRPG9 packed decode cycles, including on logical CPU 2; it was reverted. trialidct4 tested generic coefficient scanning for DC-only blocks and lost cycles on multiple real traces, so it was screened out. The same-source cab0/cab0fresh comparison exposed significant run-to-run and build-to-build variation (including a +5.31% SRPG9 packed paired median); these experiments do not establish tight equivalence bounds. Accepted reconstruction candidates use the freshly built baseline. No full-game rendering or LTO build was measured.",
    "",
    "Flat 8x8 factor precomputation had a large repeatable isolated gain and a smaller gain on the custom matrix trace. Flat 4x4 factor precomputation had repeatable isolated gains on ten flat-matrix traces but an isolated regression on the custom matrix trace; the custom-matrix full surface and packed decodes did not reproduce that slowdown. All accepted changes preserve the measured decoder allocation traffic.",
    "",
    "The chroma candidate uses the existing has_ac flag to avoid the inverse transform when only DC is present. Sampled non-skipped chroma blocks were DC-only in 81.1% to 99.1% of cases depending on the input. Isolated replay improved on all eleven inputs. The first surface batch was noisy and several medians were adverse. Repeat42 resolved Envivio and custom-matrix concerns; repeat2 on CPU 2 gave +0.66% on native Bunny, -4.29% on Bunny CAVLC and -0.55% on game CAVLC. The cab2/cab2control batch compares byte-identical executables on CPU 2, yielding +1.63% on native Bunny. The adverse medians therefore do not establish a reproducible regression beyond observed noise; this is not a tight equivalence proof. All raw samples are retained.",
    "",
    "The ten original inputs come from the [preceding entropy comparison](../windows-x86_64-20261128-entropy/README.md). bunny-custom is a new 640x360, 60-frame x264 High-profile transcode of the external Big Buck Bunny source with cqm=jvt and 8x8dct=1. All media and traces are external to Git.",
    "",
    "Cumulative surface and packed comparisons cover all eleven inputs. CPU 42 had adverse paired medians on SRPG10 surface (+2.80%), nature CAVLC surface (+9.16%), Africa surface (+20.60%) and SRPG9 packed (+16.89%), with large bidirectional outliers. The independent 11-pair CPU 2 batches gave -1.81%, -3.30%, -0.62% and -2.70%, respectively. Neither the adverse medians nor a tight equivalence margin are established as reproducible; most improvements are modest. All cumulative allocation counts and requested bytes matched exactly.",
    "",
    "## Input identity",
    "",
    "| File | Bytes | SHA-256 |",
    "|---|---:|---|",
]
for p in sorted(root.iterdir()):
    if p.suffix in (".h264", ".avcc", ".bins", ".residual", ".init", ".idct4", ".idct8", ".dq4", ".dq8", ".chroma4"):
        lines.append(f"| {p.name} | {p.stat().st_size} | `{sha(p)}` |")

def pct(a,b):
    return f"{100*(b/a-1):+.2f}%" if a else ("0%" if not b else "new")
for f in sorted(root.glob("cab*-*.csv")):
    lines += ["", f"## {f.stem}", "", f"[Raw paired samples]({f.name})", "",
        "| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |",
        "|---|---|---:|---:|---:|---:|---:|---:|"]
    cases = defaultdict(list)
    with f.open(newline="") as stream:
        for r in csv.DictReader(stream):
            cases[r["case"],r["mode"]].append(r)
    for (clip,mode), vals in cases.items():
        stages=list(dict.fromkeys(r["stage"] for r in vals))
        assert len(stages)==2
        cycles=[{r["round"]:int(r["cycles"]) for r in vals if r["stage"]==stage and r["kind"]=="cycles"} for stage in stages]
        pairs=sorted(set(cycles[0])&set(cycles[1]))
        assert len(pairs)==len(cycles[0])==len(cycles[1])
        ratios=[cycles[1][k]/cycles[0][k] for k in pairs]
        med=[stats.median(c[k] for k in pairs) for c in cycles]
        ratio=stats.median(ratios)
        alloc=[{(int(r["calls"]),int(r["bytes"])) for r in vals if r["stage"]==stage and r["kind"]=="alloc"} for stage in stages]
        if any(alloc):
            assert all(len(a)==1 for a in alloc)
            (a0,b0),(a1,b1)=[next(iter(a)) for a in alloc]
            alloc_text=f"{a0:,} / {a1:,} ({pct(a0,a1)}) | {b0:,} / {b1:,} ({pct(b0,b1)})"
        else:
            alloc_text="not instrumented | not instrumented"
        units=int(vals[0]["units"])
        lines.append(f"| {clip} | {mode} | {len(pairs)} | {med[0]:,.0f} / {med[1]:,.0f} | {100*(ratio-1):+.2f}% ({100*(min(ratios)-1):+.2f}%..{100*(max(ratios)-1):+.2f}%) | {units*1e9/med[0]:,.1f} / {units*1e9/med[1]:,.1f} ({100*(1/ratio-1):+.2f}%) | {alloc_text} |")
lines += [
    "",
    "## Final validation",
    "",
    "All-features cargo test: 100 unit tests, 4 integration tests and 7 doctests passed. No-default-features cargo check and cargo fmt check passed. The two new dequantization tests cover QP 0 through 63, flat and custom matrices, and extreme i32 coefficients. The DC shortcut uses the existing has_ac invariant and wrapping rounding; both call sites initialize the AC block to zero when has_ac is false.",
    "",
    "Final baseline/final comparison: all eleven inputs in each of surface Annex B and packed AVCC modes (22 comparisons) matched FFmpeg's visible YUV420p plane hashes in display order. Adjacent revision checks also matched. The reference hashes are:",
    "",
    "| Input | SHA-256 |",
    "|---|---|",
    "| SRPG9 | `f9301a596ac487ba19393bc4f0fe4848d4a59fbd877a100063952633a88fed90` |",
    "| SRPG10 | `fb59c6f413e15e17cccff1e43125a6758799cafd205d8d3b6a91154f15c8703e` |",
    "| Bunny | `3511282e16c625404196da6a8e3600e1c8eef7d8a4687a33694658a3e5e97056` |",
    "| Nature | `7c1208d0339e0c1749b05ecd5a30a7fe2fdb586b21d108026f8e37933246e11f` |",
    "| Envivio | `71c6c9fa9ae757032a33cc9c440318f2a4f5d9d9f78baa57b872997b091206d7` |",
    "| Interlaced | `b87aac1ea34e23a61871b2c789853d579aa5d3e2b5b44ab15655a5ba2d9270dd` |",
    "| Game CAVLC | `2ae5a960e68994f390d01884047a3f62743c6e9e8ccf287b397b7cac9f69e374` |",
    "| Bunny CAVLC | `6bec3d68d60d251a5dcc205a123a7700f6470760a0d6c7989a294f96158a1c62` |",
    "| Nature CAVLC | `5f86e1ce4609dd4da56e1d396cae38b6c6806e5e5cabc7b2e5ac5573cb4b77a0` |",
    "| Africa | `e8b847769b792e306eab67b2c5dc12489fca347a661daf62879843779754e72c` |",
    "| Bunny custom | `685261255808a496edf3767951fa80a67d564279334ef2edc798e0bffaa53537` |",
]
out.write_text("\n".join(lines)+"\n", encoding="utf-8")
