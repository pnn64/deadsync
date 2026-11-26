"""Validate and summarize independent cycle and allocation runs."""

import argparse
import csv
from collections import defaultdict
from math import comb
from pathlib import Path
from statistics import median

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("results", type=Path)
parser.add_argument("--cycle-runs", type=int, default=5)
parser.add_argument("--allocation-runs", type=int, default=2)
args = parser.parse_args()
root = args.results


def load(name: str):
    groups = defaultdict(list)
    with (root / name).open(newline="") as source:
        for row in csv.DictReader(source):
            groups[(row["stream"], row["mode"])].append(
                {key: int(value) for key, value in row.items() if key not in ("stream", "mode")}
            )
    return groups


cycle_runs = load("cycles.csv")
allocation_runs = load("allocations.csv")
assert cycle_runs.keys() == allocation_runs.keys()
streams = {stream for stream, _ in cycle_runs}
modes = {mode for _, mode in cycle_runs}
baseline_modes = {"deadlib", "ffmpeg-surface", "ffmpeg-packed"}
recycle_modes = baseline_modes | {"deadlib-recycled"}
assert len(streams) == 2
sps_modes = recycle_modes | {"deadlib-recycled-sps"}
assert modes in (baseline_modes, recycle_modes, sps_modes, sps_modes | {"deadlib-surface"})
assert len(cycle_runs) == len(streams) * len(modes)
records = []
for key in cycle_runs:
    cycles = cycle_runs[key]
    allocations = allocation_runs[key]
    assert [row["run"] for row in cycles] == list(range(1, args.cycle_runs + 1))
    assert [row["run"] for row in allocations] == list(range(1, args.allocation_runs + 1))
    assert {row["cycles"] for row in allocations} == {0}
    assert len({row["frames"] for row in cycles + allocations}) == 1
    assert len({row["luma_pixels"] for row in cycles + allocations}) == 1
    frames = cycles[0]["frames"]
    pixels = cycles[0]["luma_pixels"]
    assert frames > 0 and pixels == frames * 1920 * 1080
    a = allocations[0]
    for column in ("allocations", "reallocations", "requested_bytes"):
        assert all(a[column] == row[column] for row in allocations), (key, column)
    values = [row["cycles"] for row in cycles]
    midpoint = median(values)
    assert midpoint > 0
    records.append(
        {
            "stream": key[0],
            "mode": key[1],
            "frames": frames,
            "cycle_median": midpoint,
            "cycle_min": min(values),
            "cycle_max": max(values),
            "cycles_per_frame": midpoint / frames,
            "cycles_per_luma_pixel": midpoint / pixels,
            "frames_per_gcycle": frames * 10**9 / midpoint,
            "mpix_per_gcycle": pixels * 1000 / midpoint,
            "allocations": a["allocations"],
            "reallocations": a["reallocations"],
            "requested_bytes": a["requested_bytes"],
            "allocation_calls_per_frame": (a["allocations"] + a["reallocations"]) / frames,
            "requested_mib_per_frame": a["requested_bytes"] / frames / 2**20,
        }
    )

with (root / "summary.csv").open("w", newline="") as output:
    writer = csv.DictWriter(output, fieldnames=list(records[0]))
    writer.writeheader()
    writer.writerows(records)

def median_interval(samples):
    """Return a binomial interval for the median and its attainable coverage."""
    values = sorted(samples)
    n = len(values)
    possibilities = 2**n
    tail = 1  # At least the two sample extremes.
    drop = 0
    for k in range(1, n // 2):
        next_tail = tail + comb(n, k)
        if 40 * next_tail > possibilities:
            break
        tail = next_tail
        drop = k
    return values[drop], values[-1 - drop], 100 * (1 - 2 * tail / possibilities)


pairs = [("deadlib", "ffmpeg-packed")]
if "deadlib-surface" in modes:
    pairs.append(("deadlib-surface", "ffmpeg-surface"))
comparisons = []
for stream in sorted(streams):
    for deadlib_mode, ffmpeg_mode in pairs:
        deadlib_key = (stream, deadlib_mode)
        ffmpeg_key = (stream, ffmpeg_mode)
        samples = [
            (a["cycles"], b["cycles"])
            for a, b in zip(cycle_runs[deadlib_key], cycle_runs[ffmpeg_key])
        ]
        cycle_changes = [100 * (a / b - 1) for a, b in samples]
        throughput_changes = [100 * (b / a - 1) for a, b in samples]
        low, high, coverage = median_interval(cycle_changes)
        a = allocation_runs[deadlib_key][0]
        b = allocation_runs[ffmpeg_key][0]
        calls_a = a["allocations"] + a["reallocations"]
        calls_b = b["allocations"] + b["reallocations"]
        comparisons.append(
            {
                "stream": stream,
                "deadlib_mode": deadlib_mode,
                "ffmpeg_mode": ffmpeg_mode,
                "cycle_rounds": len(samples),
                "median_cycle_delta_pct": median(cycle_changes),
                "median_throughput_delta_pct": median(throughput_changes),
                "cycle_interval_low_pct": low,
                "cycle_interval_high_pct": high,
                "interval_coverage_pct": coverage,
                "allocation_call_delta_pct": 100 * (calls_a / calls_b - 1),
                "requested_byte_delta_pct": 100 * (a["requested_bytes"] / b["requested_bytes"] - 1),
            }
        )

with (root / "paired-summary.csv").open("w", newline="") as output:
    writer = csv.DictWriter(output, fieldnames=list(comparisons[0]))
    writer.writeheader()
    writer.writerows(comparisons)

for r in records:
    spread = (r["cycle_max"] - r["cycle_min"]) / r["cycle_median"] * 100
    print(
        f'{r["stream"]:6} {r["mode"]:14} {r["frames"]:>4} frames, '
        f'{r["cycles_per_frame"]/1e6:6.2f} Mcycles/frame, '
        f'{r["cycles_per_luma_pixel"]:5.2f} cycles/px, '
        f'{r["frames_per_gcycle"]:6.1f} frames/Gcycle, '
        f'{r["mpix_per_gcycle"]:6.1f} MPix/Gcycle, '
        f'{r["allocation_calls_per_frame"]:6.2f} calls/frame, '
        f'{r["requested_mib_per_frame"]:5.2f} MiB/frame, '
        f'{spread:4.1f}% sampled spread'
    )

for r in comparisons:
    print(
        f'{r["stream"]:6} {r["deadlib_mode"]:15} vs {r["ffmpeg_mode"]:14} '
        f'{r["median_cycle_delta_pct"]:+5.2f}% paired cycles, '
        f'{r["median_throughput_delta_pct"]:+5.2f}% paired throughput, '
        f'{r["allocation_call_delta_pct"]:+5.2f}% allocation calls, '
        f'{r["requested_byte_delta_pct"]:+5.2f}% requested bytes, '
        f'{r["interval_coverage_pct"]:.2f}% median cycle interval '
        f'[{r["cycle_interval_low_pct"]:+.2f}%, {r["cycle_interval_high_pct"]:+.2f}%]'
    )
