"""Compare two instrumented song_lua_execution binaries on explicitly named songs.

Build both binaries with identical profiles and benchmark instrumentation. Runs
are serial, alternate variant order, and fail if observed visual states differ.
No fixture discovery, builds, window, audio device or input injection is used.
"""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess


def validate_actor_counts(data, expectations, report, *, require_vertices=False):
    for prefix, expected in expectations:
        for phase in ("initial_actors", "final_actors"):
            actors = [actor for actor in data[phase]
                      if (actor.get("name") or "").startswith(prefix)]
            actual = len(actors)
            if actual != expected:
                raise RuntimeError(f"{report}: {phase} has {actual} actors named "
                                   f"{prefix}*, expected {expected}")
            if require_vertices and any(not actor.get("vertices") for actor in actors):
                raise RuntimeError(f"{report}: {phase} has an actor named "
                                   f"{prefix}* without mesh vertices")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--song", type=Path, action="append", required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=3)
    parser.add_argument("--frames", type=int, default=481)
    parser.add_argument("--expect-actors", nargs=2, action="append", default=[],
                        metavar=("PREFIX", "COUNT"),
                        help="require COUNT actors whose names start with PREFIX in "
                             "both snapshots of every run")
    parser.add_argument("--expect-meshes", nargs=2, action="append", default=[],
                        metavar=("PREFIX", "COUNT"),
                        help="require COUNT actors named PREFIX*, each with nonempty "
                             "mesh vertices in both snapshots of every run")
    args = parser.parse_args()
    if args.pairs < 1 or args.frames <= 60:
        parser.error("pairs must be positive and frames must exceed the 60-frame warmup")
    try:
        expectations = [(prefix, int(count)) for prefix, count in args.expect_actors]
        mesh_expectations = [(prefix, int(count)) for prefix, count in args.expect_meshes]
    except ValueError:
        parser.error("expected actor counts must be integers")
    if any(count < 0 for _, count in expectations + mesh_expectations):
        parser.error("expected actor counts must be nonnegative")
    args.out.mkdir(parents=True, exist_ok=True)
    binaries = {kind: getattr(args, kind).resolve() for kind in ("before", "after")}
    songs = [song.resolve() for song in args.song]
    runs = [{kind: [] for kind in binaries} for _ in songs]
    for trial in range(args.pairs):
        for index, song in enumerate(songs):
            order = ("before", "after") if trial % 2 == 0 else ("after", "before")
            for kind in order:
                report = args.out / f"{index}-{kind}-{trial + 1}.json"
                with report.with_suffix(".log").open("wb") as log:
                    subprocess.run([str(binaries[kind]), str(song), str(report.resolve()),
                                    "--benchmark", str(args.frames)],
                                   stdout=log, stderr=subprocess.STDOUT, check=True)
                data = json.loads(report.read_bytes())
                if not data["startup_smoke_passed"] or data["errors"]:
                    raise RuntimeError(f"failed gameplay execution: {report}")
                validate_actor_counts(data, expectations, report)
                validate_actor_counts(data, mesh_expectations, report, require_vertices=True)
                metrics = data["benchmark"]
                runs[index][kind].append({key: value for key, value in metrics.items() if key != "samples"})
                print(f"{song.stem} {kind} pair {trial + 1}: "
                      f"{metrics['cpu_cycles_per_update']} cycles/update", flush=True)
            fingerprints = {run["visual_sha256"] for group in runs[index].values() for run in group}
            if len(fingerprints) != 1:
                raise RuntimeError(f"observed visual states differ for {song}; inspect {args.out}")
    fields = ("cpu_cycles_per_update", "updates_per_second", "allocations_per_update",
              "reallocations_per_update", "allocated_bytes_per_update")
    summary = []
    for song, variants in zip(songs, runs):
        medians = {kind: {field: statistics.median(run[field] for run in data)
                         for field in fields if all(run[field] is not None for run in data)}
                   for kind, data in variants.items()}
        changes = {field: (medians["after"][field] / value - 1) * 100
                   for field, value in medians["before"].items() if value}
        summary.append({"song": str(song), "medians": medians, "percent_change": changes,
                        "visual_sha256": variants["before"][0]["visual_sha256"], "runs": variants})
    result = {"frames": args.frames, "warmup_frames": 60, "pairs": args.pairs,
              "expected_actors": [{"prefix": prefix, "count": count}
                                  for prefix, count in expectations],
              "expected_meshes": [{"prefix": prefix, "count": count}
                                  for prefix, count in mesh_expectations],
              "binaries": {kind: {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                           for kind, path in binaries.items()}, "workloads": summary}
    (args.out / "summary.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
