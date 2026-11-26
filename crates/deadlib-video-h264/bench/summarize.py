"""Validate and summarize independent cycle and allocation runs."""

import argparse
import csv
from collections import defaultdict
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
