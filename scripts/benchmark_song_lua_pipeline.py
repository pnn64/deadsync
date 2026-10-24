"""Compare normal-library compile_pipeline executables, preserving exact DTO snapshots."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=6)
    parser.add_argument("--samples", type=int, default=9)
    parser.add_argument("--only", action="append", metavar="WORKLOAD",
                        help="Measure only the named workload; may be repeated")
    parser.add_argument("--reverse-first", action="store_true",
                        help="Start with after/before, then alternate subsequent pairs")
    parser.add_argument("--suite", choices=("live", "startup", "actor_state", "finalization_events", "asset_strings", "sound_paths", "comparison_text", "borrowed_compat", "capture_inputs", "navigation_inputs", "state_boundaries", "shared_inputs", "timed_inputs", "string_states"), default="live")
    args = parser.parse_args()
    assert args.pairs > 0 and args.samples > 0
    root = Path(__file__).resolve().parent.parent
    binaries = {key: getattr(args, key).resolve() for key in ("before", "after")}
    binary_hashes = {key: sha(path) for key, path in binaries.items()}
    cases = [
        ("offline_basic", "offline", "tests/fixtures/song_lua/basic.lua"),
        ("live_wrappers", "live", "crates/deadsync-song-lua/tests/perf/compile_finalize.lua"),
        ("live_actors", "live", "crates/deadsync-song-lua/tests/perf/live_actors.lua"),
        ("live_two_layers", "two_live", "crates/deadsync-song-lua/tests/perf/compile_finalize.lua"),
        ("live_timing4096", "timing", "crates/deadsync-song-lua/tests/perf/compile_finalize.lua"),
        ("context_owned", "context_owned", "crates/deadsync-song-lua/tests/perf/compile_finalize.lua"),
        ("context_borrowed", "context_borrowed", "crates/deadsync-song-lua/tests/perf/compile_finalize.lua"),
        ("extract_plain128", "extract_plain", "crates/deadsync-song-lua/tests/perf/compile_finalize.lua"),
        ("extract_proxy129", "extract_proxy", "crates/deadsync-song-lua/tests/perf/compile_finalize.lua"),
    ]
    if args.suite == "startup":
        cases = cases[:5]
        cases.insert(1, ("offline_capture", "offline", "crates/deadsync-song-lua/tests/perf/startup_capture.lua"))
    if args.suite == "actor_state":
        cases = [
            ("offline_actor_state", "offline", "crates/deadsync-song-lua/tests/perf/actor_state.lua"),
            ("offline_sampling_buffers", "offline", "tests/fixtures/song_lua/sampling_buffers.lua"),
        ]
    if args.suite == "finalization_events":
        cases = [("offline_finalization_events", "offline", "crates/deadsync-song-lua/tests/perf/finalization_events.lua")]
    if args.suite == "asset_strings":
        cases = [("offline_asset_strings", "offline", "crates/deadsync-song-lua/tests/perf/asset_strings.lua"),
                 ("live_asset_strings", "live", "crates/deadsync-song-lua/tests/perf/asset_strings.lua")]
    if args.suite == "sound_paths":
        cases = [("offline_sound_paths", "offline", "crates/deadsync-song-lua/tests/perf/sound_paths.lua"),
                 ("live_sound_paths", "live", "crates/deadsync-song-lua/tests/perf/sound_paths.lua")]
    if args.suite == "comparison_text":
        cases = [("offline_comparison_text", "offline", "crates/deadsync-song-lua/tests/perf/comparison_text.lua"),
                 ("live_comparison_text", "live", "crates/deadsync-song-lua/tests/perf/comparison_text.lua")]
    if args.suite == "borrowed_compat":
        cases = [("offline_borrowed_compat", "offline", "crates/deadsync-song-lua/tests/perf/borrowed_compat.lua"),
                 ("live_borrowed_compat", "live", "crates/deadsync-song-lua/tests/perf/borrowed_compat.lua")]
    if args.suite == "capture_inputs":
        cases = [("offline_capture_inputs", "offline", "crates/deadsync-song-lua/tests/perf/capture_inputs.lua"),
                 ("live_capture_inputs", "live", "crates/deadsync-song-lua/tests/perf/capture_inputs.lua")]
    if args.suite == "navigation_inputs":
        cases = [("offline_navigation_inputs", "offline", "crates/deadsync-song-lua/tests/perf/navigation_inputs.lua"),
                 ("live_navigation_inputs", "live", "crates/deadsync-song-lua/tests/perf/navigation_inputs.lua")]
    if args.suite == "state_boundaries":
        cases = [("offline_state_boundaries", "offline", "crates/deadsync-song-lua/tests/perf/state_boundaries.lua"),
                 ("live_state_boundaries", "live", "crates/deadsync-song-lua/tests/perf/state_boundaries.lua")]
    if args.suite == "shared_inputs":
        cases = [
            ("offline_basic", "offline", "tests/fixtures/song_lua/basic.lua"),
            ("live_actor_inputs", "live", "crates/deadsync-song-lua/tests/fixtures/actor-input.lua"),
            ("live_shared_inputs", "live", "crates/deadsync-song-lua/tests/fixtures/stateful-input.lua"),
            ("offline_state_boundaries", "offline", "crates/deadsync-song-lua/tests/perf/state_boundaries.lua"),
            ("live_state_boundaries", "live", "crates/deadsync-song-lua/tests/perf/state_boundaries.lua"),
        ]
    if args.suite == "timed_inputs":
        cases = [
            ("offline_basic", "offline", "tests/fixtures/song_lua/basic.lua"),
            ("live_timed_inputs", "live", "crates/deadsync-song-lua/tests/fixtures/timed-input.lua"),
            ("live_shared_inputs", "live", "crates/deadsync-song-lua/tests/fixtures/stateful-input.lua"),
            ("offline_state_boundaries", "offline", "crates/deadsync-song-lua/tests/perf/state_boundaries.lua"),
            ("live_state_boundaries", "live", "crates/deadsync-song-lua/tests/perf/state_boundaries.lua"),
        ]
    if args.suite == "string_states":
        cases = [
            ("offline_basic", "offline", "tests/fixtures/song_lua/basic.lua"),
            ("live_string_states", "live", "crates/deadsync-song-lua/tests/fixtures/state-machine-input.lua"),
            ("live_timed_inputs", "live", "crates/deadsync-song-lua/tests/fixtures/timed-input.lua"),
            ("offline_state_boundaries", "offline", "crates/deadsync-song-lua/tests/perf/state_boundaries.lua"),
            ("live_state_boundaries", "live", "crates/deadsync-song-lua/tests/perf/state_boundaries.lua"),
        ]
    if args.only:
        selected = set(args.only)
        cases = [case for case in cases if case[0] in selected]
        missing = selected - {case[0] for case in cases}
        if missing:
            parser.error(f"Unknown workload(s) in this suite: {', '.join(sorted(missing))}")
    args.out.mkdir(parents=True, exist_ok=True)
    paths = [path for crate in ("deadlib-lua", "deadsync-song-lua", "deadsync-noteskin")
             for path in (root / "crates" / crate / "src").rglob("*.rs")]
    paths += [root / name for name in sorted({case[2] for case in cases})]
    paths += [root / name for name in ["Cargo.toml", "Cargo.lock", ".cargo/config.toml",
        "crates/deadsync-song-lua/Cargo.toml", "crates/deadsync-song-lua/examples/compile_pipeline.rs",
        "tests/support/song_lua_metrics.rs", "scripts/benchmark_song_lua_pipeline.py"]]
    if args.suite == "finalization_events":
        paths += [root / "crates/deadsync-song-lua/tests/perf" / name
                  for name in ["finalization-a 2x6.png", "finalization-b 2x6.png"]]
    if args.suite == "asset_strings":
        paths += [root / "crates/deadsync-song-lua/tests/perf" / name
                  for name in ["finalization-a 2x6.png", "asset-string.wav"]]
    if args.suite == "sound_paths":
        paths += [root / "crates/deadsync-song-lua/tests/perf/asset-string.wav"]
    if args.suite == "comparison_text":
        paths += [root / "crates/deadsync-song-lua/tests/perf/finalization-a 2x6.png"]
    if args.suite == "capture_inputs":
        paths += [root / "crates/deadsync-song-lua/tests/perf/asset-string.wav"]
    sources = {path.relative_to(root).as_posix(): sha(path) for path in paths}
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    records = []
    snapshots = {}
    for trial in range(1, args.pairs + 1):
        for name, mode, source in cases:
            for variant in (("before", "after") if bool(trial % 2) != args.reverse_first else ("after", "before")):
                report = args.out / f"{name}-{variant}-{trial}.json"
                with report.with_suffix(".log").open("wb") as log:
                    subprocess.run([str(binaries[variant]), mode, str(root/source), str(report.resolve()),
                                    str(args.samples)], cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True)
                data = json.loads(report.read_bytes())["measurement"]
                snapshot = data.pop("snapshot")
                if name in snapshots:
                    assert snapshot == snapshots[name], f"complete output changed for {name}; see {report}"
                else:
                    snapshots[name] = snapshot
                assert len(data["samples"]) == args.samples
                data.update(workload=name, variant=variant, pair=trial,
                            snapshot_sha256=hashlib.sha256(snapshot.encode()).hexdigest(),
                            report_sha256=sha(report), report=str(report.resolve()))
                records.append(data)
                print(f"{trial}/{args.pairs} {name} {variant}", flush=True)
    summary = []
    for name, mode, source in cases:
        values = {}
        for variant in ("before", "after"):
            entries = [(sample, row["operations_per_sample"]) for row in records
                       if row["workload"] == name and row["variant"] == variant for sample in row["samples"]]
            values[variant] = {key: statistics.median(sample[key]/count for sample, count in entries)
                               for key in ("wall_seconds", "thread_cpu_cycles", "allocations", "reallocations", "allocated_bytes")}
            values[variant]["operations_per_second"] = 1/values[variant]["wall_seconds"]
        values.update(workload=name, mode=mode, input=source,
                      throughput_gain_percent=100*(values["before"]["wall_seconds"]/values["after"]["wall_seconds"]-1),
                      cycle_reduction_percent=100*(1-values["after"]["thread_cpu_cycles"]/values["before"]["thread_cpu_cycles"]))
        summary.append(values)
    assert all(sha(path) == binary_hashes[key] for key,path in binaries.items())
    assert all(sha(root/name) == digest for name,digest in sources.items())
    assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip() == head
    result = dict(worktree_head=head, binaries={key:{"path":str(path),"sha256":binary_hashes[key]} for key,path in binaries.items()},
                  source_sha256=sources, suite=args.suite, pairs=args.pairs, reverse_first=args.reverse_first, samples_per_variant_per_case=args.pairs*args.samples,
                  rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                  system=platform.platform(), logical_processors=os.cpu_count(), logical_cpu=2,
                  summary=summary, records=records,
                  validation="Every complete DTO snapshot is compared exactly across all runs; snapshots and sample metrics are retained uncompressed in each run report.")
    (args.out / "summary.json").write_text(json.dumps(result, indent=2)+"\n", encoding="utf-8")
    for row in summary:
        print(row["workload"], f"throughput {row['throughput_gain_percent']:+.1f}%; cycles {-row['cycle_reduction_percent']:+.1f}%")


if __name__ == "__main__":
    main()
