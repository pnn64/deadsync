import csv, hashlib, pathlib, statistics as stats, struct, sys
from collections import defaultdict, Counter

root = pathlib.Path(__file__).resolve().parent
out = pathlib.Path(sys.argv[1])
revisions = sys.argv[2:]
sources = {
    "bunny.mp4": "https://raw.githubusercontent.com/chthomos/video-media-samples/master/big-buck-bunny-1080p-30sec.mp4",
    "africa.mov": "https://samples.ffmpeg.org/V-codecs/h264/bbc-africa_m720p.mov",
    "nature.h264": "https://samples.ffmpeg.org/V-codecs/h264/nature_704x576_25Hz_1500kbits.h264",
    "envivio.mp4": "https://samples.ffmpeg.org/V-codecs/h264/envivio-h264.mp4",
    "interlaced.mp4": "https://samples.ffmpeg.org/V-codecs/h264/interlaced_crop.mp4",
}
lines = [
    "# H.264 entropy decoding revision comparison (2026-11-28)",
    "",
    "Windows x64, Xeon E5-2696 v4, logical CPU 42 (CPU 2 in repeat2), Rust 1.100.0. All measured binaries use identical direct rustc settings: opt-level=3, 16 codegen units for the decoder, default target CPU, std and surface, no LTO. QueryThreadCycleTime measures the pinned serial decoding thread; allocation counting runs separately. Each sample begins with fresh decoder and lazy tables. Allocation calls include alloc, alloc_zeroed and realloc; requested bytes count traffic rather than peak memory. Other processes were active on this host.",
    "",
    "The ten video inputs include the three previous game samples, five independent native internet streams and two CAVLC transcodes of internet footage. The files, excerpts and sampled entropy traces are temporary and outside the repository. No sample bytes are included in this report or committed.",
    "",
    "The pipeline includes NAL parsing, RBSP extraction, SPS parsing, decoding, display ordering, output consumption, draining and destruction. The surface mode uses SPS reorder depth; packed mode uses the default depth. Isolated benchmarks replay every 32nd call of each operation from an instrumented **baseline** decoder, with original alignment, context and up to 256 subsequent RBSP bytes. Replay setup is outside the measurement. Each batch repeats the trace 64 times (once in the cold batch). Both stages consume identical traces. This is a sampled operation replay, not an estimate of the percentage of total decode time.",
    "",
    "Source revisions: " + "; ".join(f"pass{i} = `{rev}`" for i, rev in enumerate(revisions)) + ".",
    "The first pass0/pass1 full-decode comparison (without decodeonly in the filename) linked the replay code into the executable. It was retained as exploratory evidence. Repeat batches on two logical CPUs suggested a small SRPG9 regression; the dedicated decode-only harness excludes all replay code and is used in the decodeonly42 batches and every later comparison. The dedicated comparisons put CABAC control medians within the observed noise, with no established reproducible regression. This harness distinction is material; the initial samples are not deleted or silently substituted. trialbits is a rejected fixed-width bit-read prototype, reverted before the next change.",
    "",
    "Cycle and throughput figures below are individual medians; percentage changes use medians of paired ratios and may differ from ratios of individual medians. Negative cycle and allocation changes are improvements. Paired ranges expose variance. Allocation counts and requested bytes are the absolute totals per batch, including lazy table initialization. Separate fresh-process allocation runs must agree. Timing and allocation signatures and operation counts must agree between stages. FFmpeg 7.1 (Gyan full build) provides independent output hashes.",
    "",
    "The stack-residual change removes one heap allocation per nonempty residual block. The Exp-Golomb fast path consumes short codewords from a word and retains the original scalar fallback. The static LUT change packs coefficient entries into two bytes and eliminates the runtime table builders and OnceLock access. Its cold isolated cycle results are mixed, including regressions on Bunny and Africa, while the warm replay is effectively unchanged. The full pipeline sees reduced heap traffic and no reproducible cycle regression in the dedicated harness. The static tables increase the decode cycle executable from 1,656,832 to 1,835,008 bytes (+178,176 bytes). This is an allocation/code-simplicity tradeoff, not a claim that the isolated cold path is faster.",
    "",
    "The cumulative pass0/pass3 surface batch was noisy on Bunny CAVLC (+14.85% paired median, range -10.64%..+46.13%); a separate 11-pair repeat was consistently faster (-11.04%, range -12.99%..-7.83%). Both are included. Packed output independently confirmed the improvement (-11.41%). The interlaced control repeat was +0.42% with mixed signs. Native CABAC control performance is considered inconclusive rather than claiming a speedup or a proven equivalence margin.",
    "",
    "## External media provenance",
    "",
    "Big Buck Bunny: (c) copyright 2008, Blender Foundation / www.bigbuckbunny.org, CC BY 3.0. Original internet files were used locally for testing only.",
    "FFmpeg emitted truncated SEI diagnostics on the BBC Africa source, but the full visible-plane hashes matched on every accepted revision.",
    "",
    "| Source file | Download URL | Bytes | SHA-256 |",
    "|---|---|---:|---|",
]
def sha(path):
    with path.open("rb") as f: return hashlib.file_digest(f,"sha256").hexdigest()

for name,url in sources.items():
    p=root/name
    lines.append(f"| {name} | {url} | {p.stat().st_size} | `{sha(p)}` |")
lines += ["", "## Input identity", "", "| Input | Bytes | SHA-256 |", "|---|---:|---|"]
for p in sorted(list(root.glob("*.h264")) + list(root.glob("*.avcc"))):
    if p.name in sources: continue
    lines.append(f"| {p.name} | {p.stat().st_size} | `{sha(p)}` |")
lines += ["", "## Isolated trace identity", "", "| Input | Residual calls | UE calls | Fixed-width reads | SHA-256 |", "|---|---:|---:|---:|---|"]
for p in sorted(root.glob("*.trace")):
    data=p.read_bytes()
    counts=Counter()
    i=0
    while i<len(data):
        tag,_,_,_,n=struct.unpack_from("<BiBBH",data,i)
        counts[tag]+=1
        i+=9+n
    assert i==len(data)
    lines.append(f"| {p.name} | {counts[0]} | {counts[1]} | {counts[2]} | `{sha(p)}` |")
def pct(a,b):
    if a==0: return "0%" if b==0 else "new"
    return f"{100*(b/a-1):+.2f}%"
for f in sorted(root.glob("pass*-*.csv")):
    lines += ["",f"## {f.stem}","",f"[Raw paired samples]({f.name})","",
        "Throughput: sampled operations or decoded frames per billion CPU cycles.","",
        "| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |",
        "|---|---|---:|---:|---:|---:|---:|---:|"]
    cases=defaultdict(list)
    with f.open(newline="") as stream:
        for r in csv.DictReader(stream): cases[r["case"],r["mode"]].append(r)
    for (clip,mode),vals in cases.items():
        stages=list(dict.fromkeys(r["stage"] for r in vals))
        assert len(stages)==2
        cycles=[{r["round"]:int(r["cycles"]) for r in vals if r["stage"]==stage and r["kind"]=="cycles"} for stage in stages]
        pairs=sorted(set(cycles[0])&set(cycles[1]))
        assert len(pairs)==len(cycles[0])==len(cycles[1])
        ratios=[cycles[1][k]/cycles[0][k] for k in pairs]
        med=[stats.median(c[k] for k in pairs) for c in cycles]
        ratio=stats.median(ratios)
        alloc=[{(int(r["calls"]),int(r["bytes"])) for r in vals if r["stage"]==stage and r["kind"]=="alloc"} for stage in stages]
        assert all(len(a)==1 for a in alloc)
        (a0,b0),(a1,b1)=[next(iter(a)) for a in alloc]
        units=int(vals[0]["units"])
        lines.append(f"| {clip} | {mode} | {len(pairs)} | {med[0]:,.0f} / {med[1]:,.0f} | {100*(ratio-1):+.2f}% ({100*(min(ratios)-1):+.2f}%..{100*(max(ratios)-1):+.2f}%) | {units*1e9/med[0]:,.1f} / {units*1e9/med[1]:,.1f} ({100*(1/ratio-1):+.2f}%) | {a0:,} / {a1:,} ({pct(a0,a1)}) | {b0:,} / {b1:,} ({pct(b0,b1)}) |")
lines += ["", "## Bit-exact output identity", "",
    "All accepted adjacent revision pairs and the baseline/final comparison are checked against FFmpeg's SHA-256 of visible YUV420p planes in display order. The surface harness covers all ten inputs; the packed harness covers the four CAVLC inputs.",
    "", "| Input | Frames | SHA-256 |", "|---|---:|---|",
    "| SRPG9 | 45 | `f9301a596ac487ba19393bc4f0fe4848d4a59fbd877a100063952633a88fed90` |",
    "| SRPG10 | 45 | `fb59c6f413e15e17cccff1e43125a6758799cafd205d8d3b6a91154f15c8703e` |",
    "| SRPG10 CAVLC | 90 | `2ae5a960e68994f390d01884047a3f62743c6e9e8ccf287b397b7cac9f69e374` |",
    "| Bunny CAVLC | 90 | `6bec3d68d60d251a5dcc205a123a7700f6470760a0d6c7989a294f96158a1c62` |",
    "| Nature CAVLC | 60 | `5f86e1ce4609dd4da56e1d396cae38b6c6806e5e5cabc7b2e5ac5573cb4b77a0` |",
    "| BBC Africa | 45 | `e8b847769b792e306eab67b2c5dc12489fca347a661daf62879843779754e72c` |",
    "| Big Buck Bunny | 36 | `3511282e16c625404196da6a8e3600e1c8eef7d8a4687a33694658a3e5e97056` |",
    "| Nature | 60 | `7c1208d0339e0c1749b05ecd5a30a7fe2fdb586b21d108026f8e37933246e11f` |",
    "| Envivio | 60 | `71c6c9fa9ae757032a33cc9c440318f2a4f5d9d9f78baa57b872997b091206d7` |",
    "| Interlaced crop | 60 | `b87aac1ea34e23a61871b2c789853d579aa5d3e2b5b44ab15655a5ba2d9270dd` |",
]
out.write_text("\n".join(lines)+"\n",encoding="utf-8")
