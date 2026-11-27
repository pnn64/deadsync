import csv
import sys
from collections import defaultdict
from pathlib import Path
from statistics import median


def read_samples(path):
    samples = defaultdict(dict)
    with path.open(newline="") as stream:
        for row in csv.DictReader(stream):
            key = row.pop("clip"), row.pop("engine")
            run = int(row.pop("round"))
            assert run not in samples[key]
            samples[key][run] = {name: int(value) for name, value in row.items()}
    return samples


def write_rows(path, rows):
    with path.open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)


def main():
    out = Path(sys.argv[1])
    cycle_runs, allocation_runs = map(int, sys.argv[2:4])
    codec = sys.argv[4].lower() if len(sys.argv) > 4 else "mpeg2"
    cycles = read_samples(out / "cycles.csv")
    allocations = read_samples(out / "allocations.csv")
    expected_frames = {
        "mpeg1": {"company": 181, "intro": 941, "nexuswave": 299},
        "mpeg2": {"dix": 865, "lights": 1553},
        "mpeg4": {"logo": 900, "progressive": 10},
        "mp42": {"A-Short_People_Got_No_Reason_To_Live": 2206, "banner": 69, "mp42": 10},
        "vp9": {"Countdown": 426, "Film": 603, "indexed": 9},
    }[codec]
    expected_keys = {(clip, engine) for clip in expected_frames for engine in ("deadlib", "ffmpeg")}
    assert cycles.keys() == allocations.keys() == expected_keys

    summary = []
    paired = []
    for clip, frames in expected_frames.items():
        for engine in ("deadlib", "ffmpeg"):
            cycle_rows = cycles[clip, engine]
            alloc_rows = allocations[clip, engine]
            assert cycle_rows.keys() == set(range(cycle_runs))
            assert alloc_rows.keys() == set(range(allocation_runs))
            assert all(row["frames"] == frames for row in alloc_rows.values())
            frame_totals = {row["frames"] for row in cycle_rows.values()}
            assert len(frame_totals) == 1
            cycle_frames = frame_totals.pop()
            assert cycle_frames >= frames and cycle_frames % frames == 0
            assert cycle_frames == cycles[clip, "deadlib"][0]["frames"]
            decodes = cycle_frames // frames
            if decodes != 1:
                assert codec == "vp9" and clip == "indexed"
            readings = [row["cycles"] for row in cycle_rows.values()]
            if decodes != 1:
                readings = [reading / decodes for reading in readings]
            mid = median(readings)
            counts = {(row["allocations"], row["reallocations"], row["requested_bytes"])
                      for row in alloc_rows.values()}
            assert len(counts) == 1, (clip, engine, counts)
            calls, reallocs, size = counts.pop()
            summary.append(dict(
                clip=clip, engine=engine, frames=frames,
                median_cycles=mid, min_cycles=min(readings), max_cycles=max(readings),
                million_cycles_per_frame=round(mid / frames / 1e6, 6),
                frames_per_billion_cycles=round(frames * 1e9 / mid, 4),
                allocations=calls, reallocations=reallocs, requested_bytes=size,
                calls_per_frame=round((calls + reallocs) / frames, 6),
                requested_kib_per_frame=round(size / frames / 1024, 6),
            ))
        ours, ffmpeg = cycles[clip, "deadlib"], cycles[clip, "ffmpeg"]
        ratios = [ours[run]["cycles"] / ffmpeg[run]["cycles"] for run in range(cycle_runs)]
        paired.append(dict(
            clip=clip, rounds=cycle_runs,
            median_cycle_ratio=round(median(ratios), 6),
            min_cycle_ratio=round(min(ratios), 6),
            max_cycle_ratio=round(max(ratios), 6),
            median_throughput_ratio=round(median(1 / ratio for ratio in ratios), 6),
        ))

    write_rows(out / "summary.csv", summary)
    write_rows(out / "paired-summary.csv", paired)
    for row in paired:
        print(f"{row['clip']}: median paired Deadlib/FFmpeg cycle ratio "
              f"{row['median_cycle_ratio']:.4f}")


if __name__ == "__main__":
    main()
