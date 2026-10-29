"""Append the measured report using exact retained summaries and UTF-8 Git history."""
import csv
import re
from pathlib import Path
import statistics
import subprocess

EVIDENCE = Path(__file__).resolve().parent
ROOT = EVIDENCE.parents[4]
PNG = ROOT / "crates/deadlib-image/png"
BASE = "8314615bc"


def rows(name):
    with (EVIDENCE / name).open(encoding="utf-8", newline="") as stream:
        return list(csv.DictReader(stream, delimiter="\t"))


def component(name, case, operation):
    return next(r for r in rows(name + "-summary.tsv") if r["case"] == case and r["operation"] == operation)


def counts(name, expected_suites):
    matches = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored",
                         (EVIDENCE / name).read_text(encoding="utf-8"))
    assert len(matches) == expected_suites
    totals = tuple(sum(int(r[i]) for r in matches) for i in range(3))
    assert totals[1] == 0
    return totals


def main():
    release = counts("final-tests.log", 10)
    debug = counts("debug.log", 10)
    library_release = counts("library-release.log", 1)
    library_debug = counts("library-debug.log", 1)
    diff = subprocess.check_output(["git", "diff", "--numstat", BASE, "--", "crates/deadlib-image/png/src"], cwd=ROOT, text=True)
    additions, deletions = (sum(int(line.split()[i]) for line in diff.splitlines()) for i in [0, 1])
    linked = rows("in-process-summary.tsv")
    by_case = {r["case"]: r for r in linked}
    median = statistics.median(float(r["cpu_throughput"]) for r in linked)
    case = "first-c6-d16-4097x1-i0-"
    body = """## First-row encoder predictors and adaptive ties (2026-10-29)

Three safe Rust changes build on `8314615bc`:

1. Buffered first rows use zero-upper predictor kernels: Up equals None,
   Paeth equals Sub, and Average subtracts half the left byte. One-row images
   omit the previous buffer. Pass initialization no longer explicitly clears
   that buffer, and the last row no longer swaps it.
2. Streaming encoding uses the same first-row kernels. A single-row pass
   omits previous-row allocation/resizing; later passes allocate it when needed.
   Explicit pass-start clears and last-row swaps are removed.
3. Adaptive selection skips Up and Paeth on each pass's first row. Their
   bytes and scores equal the already evaluated None and Sub candidates, so
   they cannot win the existing strict-score tie rule. Other rows keep all
   five candidates and their original order.

The changes reuse the constant filter kernels and add a first-row Average
kernel with no upper-row reads. Native packing, filtered bytes, selected wire
filter values, compression/framing and writer failure behavior are preserved.
Encoder memory statistics truthfully decrease with omitted buffers.

### Component comparisons

The following complete Stored PNG operations use the same RGBA16 4097x1 input.
Each component compares its immutable checkpoint with the immediately preceding
checkpoint: four alternating processes per version, three warmups and seven
CPU-median batches per process. Values are medians across four processes.

| Component / operation | Before cycles | After cycles | CPU throughput | Allocation calls |
| --- | ---: | ---: | ---: | ---: |
"""
    for title, comparison, suffix, operation in [
        ("Buffered first row, Paeth", "buffered-paired", "f4", "encode"),
        ("Streaming first row, Paeth", "writer-paired", "f4", "encode-writer"),
        ("Adaptive duplicate pruning, buffered", "adaptive-paired", "f5", "encode"),
        ("Adaptive duplicate pruning, writer", "adaptive-paired", "f5", "encode-writer"),
    ]:
        r = component(comparison, case + suffix, operation)
        body += (f"| {title} | {float(r['before_cycles']):,.0f} | {float(r['after_cycles']):,.0f} | "
                 f"{float(r['cpu_throughput']):.3f}x | {r['before_allocations']} -> {r['after_allocations']} |\n")
    buffered = component("buffered-paired", case + "f4", "encode")
    writer = component("writer-paired", case + "f4", "encode-writer")
    body += (f"\nBuffered Paeth requested bytes decrease from {int(buffered['before_requested_bytes']):,} "
             f"to {int(buffered['after_requested_bytes']):,}; writer requests decrease from "
             f"{int(writer['before_requested_bytes']):,} to {int(writer['after_requested_bytes']):,}. "
             "Each removes one 32,776-byte previous row. Allocation probes are outside timing.\n")
    body += """
### Combined results and controls

An additional caller links the immutable baseline and all three changes into
one executable. It requires complete buffered PNGs and complete writer PNGs to
match byte-for-byte, then checks decoded native samples before timing. Fifty
fixtures produce 100 API comparisons per process. Each of three processes
alternates versions within twelve batches, discards the first batch and retains
eleven. Wide single rows use 128 iterations, small widths 512 and tall controls
six. Writer timing blackboxes both output and returned EncodeStats.

| Linked comparison | Before cycles | Selected cycles | CPU throughput |
| --- | ---: | ---: | ---: |
"""
    for title, name in [
        ("RGBA16 4097x1 Paeth buffered", case + "f4-buffered"),
        ("RGBA16 4097x1 Paeth writer", case + "f4-writer"),
        ("RGBA16 4097x1 adaptive buffered", case + "f5-buffered"),
        ("RGBA16 4097x1 adaptive writer", case + "f5-writer"),
        ("RGBA8 33x9 Adam7 adaptive buffered", "first-c6-d8-33x9-i1-f5-buffered"),
        ("RGBA8 33x9 Adam7 adaptive writer", "first-c6-d8-33x9-i1-f5-writer"),
        ("RGBA8 33x9 plain adaptive buffered", "first-c6-d8-33x9-i0-f5-buffered"),
        ("Gray8 4097x1 Up writer", "first-c0-d8-4097x1-i0-f2-writer"),
        ("Gray8 4097x1 None writer", "first-c0-d8-4097x1-i0-f0-writer"),
        ("Gray8 513x257 None buffered", "first-c0-d8-513x257-i0-f0-buffered"),
        ("RGBA16 513x257 adaptive buffered", "first-c6-d16-513x257-i0-f5-buffered"),
        ("RGBA16 513x257 adaptive writer", "first-c6-d16-513x257-i0-f5-writer"),
    ]:
        r = by_case[name]
        body += f"| {title} | {float(r['before_cycles']):,.0f} | {float(r['after_cycles']):,.0f} | {float(r['cpu_throughput']):.3f}x |\n"
    tall = [float(r["cpu_throughput"]) for r in linked if "513x257" in r["case"]]
    body += (f"\nThe median of the 100 linked API ratios is {median:.3f}x; it is not a\n"
             "weighted corpus throughput. The twelve tall-control ratios range from\n"
             f"{min(tall):.3f}x to {max(tall):.3f}x. The small plain adaptive buffered control is\n"
             "about 8.2% slower and the wide gray Up writer about 5.7% slower. The\n"
             "separate-process small plain adaptive comparison is approximately neutral.\n"
             "Both datasets remain visible. These results establish targeted first-row\n"
             "and storage benefits; they do not establish faster encoding for every input.\n")
    body += """
All fifty linked writer-owned peak-buffer probes agree across three processes
and never increase. RGBA16 4097x1 Paeth drops from 233,769 to 200,993 owned
bytes. Tiny RGBA8 adaptive drops from 135,472 to 135,464 bytes, reflecting the
streaming Vec's minimum capacity. Every pipeline case also has nonincreasing
allocation calls, reallocations and requested bytes across retained components.

### Scope and verification

`experiments/first-predictors-20261029` retains four frozen checkpoints and four
alternating comparisons: 32 complete pipeline processes with 228 API operations
each. The corpus covers gray8, RGBA8, RGBA16 and packed gray2; 4097x1 and 33x9
plain/Adam7 cases; all six filters; native 513x257 None/adaptive controls and
RGBA8 1x1 cases. Every process checks complete independently modeled filtered
bytes and decoded native samples outside timing. The new `encoder-first` stage
reuses the existing fixture/oracle/allocator harness. Source, harness, profile,
toolchain, commands, logs, binaries, filtered fingerprints and allocation counts
are retained and checked. Cumulative patches are relative to `8314615bc`.

All cases use Stored compression; Fast compression has correctness coverage
without a throughput claim here. Thread CPU cycles and logical CPU2 affinity
are used. The host, frequency and cache state are uncontrolled. This task's
builds, timings and correctness jobs are serialized. Pipeline runs request
three iterations, with a minimum of 2,048 for filtered outputs below 4,096
bytes. The actual iteration count is recorded in each TSV. Linked callers
supplement the process comparisons with more iterations for short operations.

Local `measure.py` forces source timestamps forward and requires an actual
library rebuild before freezing a binary. Caller logs have a distinct
`-caller-build` suffix. Measurement records and output files are immutable.
`protocol.json` describes the individual scopes; `summary.py` recomputes linked
medians, and `write_report.py` renders these tables from the retained data.
"""
    body += (f"\nSelected source passes {release[0] + library_release[0]} release tests and "
             f"{debug[0] + library_debug[0]} debug tests: {release[0]}/{debug[0]} integration\n"
             f"tests across ten suites, plus {library_release[0]} library oracles in each profile. "
             f"All {debug[2]} optional C\n"
             "checks run in release and remain ignored in debug. Strict all-target\n"
             "Clippy and scoped formatting pass. Checks cover 17,280 independently\n"
             "modeled filtered outputs, all native formats/filter types/Adam7 tails,\n"
             "1,620 C encoder references and 3,240 corresponding native encodings,\n"
             "8,640 native APNG files, deterministic ties, native input/resource\n"
             "validation, every writer failure prefix and PNG/APNG recovery/order rules.\n"
             "The existing scalar unit oracle now also checks first-row bytes and\n"
             "scores in scored and unscored modes. Independent integration/support/\n"
             "fixture/oracle sources remain unchanged.\n")
    body += (f"\nOnly two production files change: {additions} added and {deletions} removed lines,\n"
             f"net +{additions - deletions}, including the unit-oracle assertions. Removed execution\n"
             "includes previous-row allocations, repeated clears, last-row swaps,\n"
             "zero-upper Paeth/Up work and two redundant adaptive candidate scans per\n"
             "pass. No public API, runtime dependency or unsafe code is added.\n\n"
             "`final-validation.json` and `library-validation.json` retain six successful\n"
             "commands and exact tested identities. `audit.py` verifies all four\n"
             "checkpoints, 32 pipeline processes, three linked processes, fifty memory\n"
             "probes, source/support identities, logs and recomputed summaries.\n"
             "`audit.json` records evidence and normalized report hashes before commit.\n")
    base = subprocess.check_output(["git", "show", BASE + ":crates/deadlib-image/png/BENCHMARKS.md"], cwd=ROOT)
    output = base.rstrip() + b"\n\n" + body.encode("utf-8")
    destination = PNG / "BENCHMARKS.md"
    current = destination.read_bytes().replace(b"\r\n", b"\n")
    assert current in [base, output]
    destination.write_bytes(output)
    print("Rendered measured component/control tables and exact validation counts.")


if __name__ == "__main__":
    main()
