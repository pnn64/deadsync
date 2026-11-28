import csv, hashlib, pathlib, statistics as s, sys
from collections import defaultdict

root = pathlib.Path(__file__).resolve().parent
out = pathlib.Path(sys.argv[1])
files = sorted(root.glob("stage*-stage*.csv"))
lines = [
    "# H.264 parsing and CAVLC revision comparison (2026-11-28)",
    "",
    "Host: Intel Xeon E5-2696 v4, Windows x64, logical CPU 42; rustc 1.100.0. "
    "Both revisions use the same direct release rustc settings (opt-level=3, 16 codegen units, default target CPU, std and surface enabled, no LTO). FFmpeg 7.1 (Gyan full build) provided input preparation and independent output hashes. "
    "The benchmark uses QueryThreadCycleTime on one pinned decoding thread; cycles and allocation traffic are measured in separate executables. "
    "One invocation starts with fresh decoder and lazy tables. Allocation calls include alloc, alloc_zeroed and realloc; bytes are requested traffic rather than peak memory.",
    "",
    "The two 1080p samples are the first 45 coded pictures of the bundled SRPG9 and SRPG10 game videos. "
    "The CAVLC sample is a 90-picture 640x360 baseline-profile transcode of the real SRPG10 video. "
    "Annex B and four-byte AVCC inputs contain the same NALs. The pipeline includes framing, RBSP extraction, SPS parsing, picture decoding, display ordering, output consumption, draining, and destruction. "
    "The packed output uses the default reorder depth, the native surface output uses the signaled SPS depth. "
    "Parsing repeats 64 times per sample, normalized to one parse; the peek microbenchmark cycles through CAVLC lookup widths on the real compressed bytes. "
    "That microbenchmark is a representative operation mix, not an instrumented replay of the exact decoder call trace.",
    "",
    "Stage 0 = original source at af1cf3ff8; stage 1 = b03dbf04e; stage 2 = b409c4d65; stage 3 = " + sys.argv[2] + ".",
    "",
    "Each comparison alternates the order of the two binaries across rounds. Absolute cycle and throughput figures are individual medians; percentage changes are medians of paired ratios, so they need not equal the ratio of individual medians. "
    "The full paired range is provided to show variance. Allocation counts and requested bytes were unchanged (0% change) across two fresh-process runs in every comparison. "
    "The three changes introduce no unsafe code or new dependencies. High-profile controls use CABAC; the third change primarily benefits CAVLC. "
    "These measurements establish the decoder pipeline behavior for these inputs/build settings, not the complete DeadSync render loop or every possible stream/platform. Other builds were active on the host; large outliers and small or contradictory cycle changes are inconclusive.",
    "",
    "## Input identity",
    "",
    "| File | Bytes | SHA-256 |",
    "|---|---:|---|",
]
for p in sorted(list(root.glob("*.h264")) + list(root.glob("*.avcc"))):
    lines.append(f"| {p.name} | {p.stat().st_size} | `{hashlib.sha256(p.read_bytes()).hexdigest()}` |")

for f in files:
    lines += ["",f"## {f.stem}","",f"[Raw paired samples]({f.name})","",
        "Throughput units: bytes/billion cycles for parser rows, peeks/billion cycles for peek rows, frames/billion cycles for decoding rows.","",
        "| Input | Mode | Pairs | Cycles before → after | Paired cycles Δ (min…max) | Throughput before → after (paired Δ) | Allocation calls before → after | Requested bytes before → after |",
        "|---|---|---:|---:|---:|---:|---:|---:|"]
    cases = defaultdict(list)
    with f.open(newline="") as stream:
        for r in csv.DictReader(stream):
            cases[r["case"],r["mode"]].append(r)
    for (clip,mode), vals in cases.items():
        stages=list(dict.fromkeys(r["stage"] for r in vals))
        assert len(stages)==2
        cycles=[{r["round"]:int(r["cycles"])/int(r["repeats"]) for r in vals if r["stage"]==stage and r["kind"]=="cycles"} for stage in stages]
        pairs=sorted(set(cycles[0]) & set(cycles[1]))
        ratios=[cycles[1][k]/cycles[0][k] for k in pairs]
        med=[s.median(c[k] for k in pairs) for c in cycles]
        ratio=s.median(ratios)
        alloc=[{(int(r["calls"])/int(r["repeats"]),int(r["bytes"])/int(r["repeats"])) for r in vals if r["stage"]==stage and r["kind"]=="alloc"} for stage in stages]
        assert all(len(a)==1 for a in alloc)
        (a0,b0),(a1,b1)=[next(iter(a)) for a in alloc]
        assert (a0,b0) == (a1,b1)
        units=int(vals[0]["units"])/int(vals[0]["repeats"])
        lines.append(f"| {clip} | {mode} | {len(pairs)} | {med[0]:,.0f} → {med[1]:,.0f} | {100*(ratio-1):+.2f}% ({100*(min(ratios)-1):+.2f}%…{100*(max(ratios)-1):+.2f}%) | {units*1e9/med[0]:,.1f} → {units*1e9/med[1]:,.1f} ({100*(1/ratio-1):+.2f}%) | {a0:,.0f} → {a1:,.0f} | {b0:,.0f} → {b1:,.0f} |")
lines += ["","## Output identity","",
    "Each stage compared the appropriate packed and surface output against the previous stage and FFmpeg SHA-256 of visible YUV420p planes in display order:",
    "",
    "| Clip | Frames | SHA-256 |","|---|---:|---|",
    "| SRPG9 | 45 | `f9301a596ac487ba19393bc4f0fe4848d4a59fbd877a100063952633a88fed90` |",
    "| SRPG10 | 45 | `fb59c6f413e15e17cccff1e43125a6758799cafd205d8d3b6a91154f15c8703e` |",
    "| CAVLC | 90 | `2ae5a960e68994f390d01884047a3f62743c6e9e8ccf287b397b7cac9f69e374` |",
    "",
    "## Validation",
    "",
    "Cargo all-features tests: 96 unit, 4 integration and 7 documentation tests passed. The no-default-features check and cargo fmt check passed.",
    "",
    "The start-code change skips nonzero words while preserving the original prefix search order. The EPB change removes the second prefix scan and byte-at-a-time copy; unchanged spans are copied directly. The bit peek change eliminates per-bit extraction in the common case while preserving the old zero-padding fallback. Boundary and exhaustive short-pattern tests cover the new paths.",
    ""]
out.write_text("\n".join(lines), encoding="utf-8")
