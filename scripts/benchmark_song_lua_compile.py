"""Measure frozen/current compile finalization or startup preparation scopes."""
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
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=6)
    parser.add_argument("--suite", choices=("finalize", "startup", "vm_reads", "actor_state", "ignored_args", "prefix_args", "method_storage"), default="finalize")
    args = parser.parse_args()
    assert args.rounds > 0
    root = Path(__file__).resolve().parent.parent
    binary = args.binary.resolve()
    binary_sha = sha(binary)
    paths = sorted((root / "crates/deadsync-song-lua/src").rglob("*.rs"))
    paths += [root / name for name in [
        "crates/deadsync-song-lua/tests/perf/compile_finalize.rs",
        "crates/deadsync-song-lua/tests/perf/compile_finalize_before.rs",
        "crates/deadsync-song-lua/tests/perf/compile_finalize.lua",
        "crates/deadsync-song-lua/tests/perf/live_actors.lua",
        "crates/deadsync-song-lua/tests/perf/draw_capture.rs",
        "tests/fixtures/song_lua/basic.lua", "tests/support/perf.rs",
        "crates/deadsync-song-lua/Cargo.toml", "Cargo.toml", "Cargo.lock",
        ".cargo/config.toml", "scripts/benchmark_song_lua_compile.py",
    ]]
    sources = {path.relative_to(root).as_posix(): sha(path) for path in paths}
    if args.suite == "startup":
        for name in ["native_function.rs", "native_function_before.rs", "startup_capture.rs", "startup_capture_before.rs"]:
            path = root / "crates/deadsync-song-lua/tests/perf" / name
            sources[path.relative_to(root).as_posix()] = sha(path)
    if args.suite == "vm_reads":
        for name in ["vm_reads_actor.rs", "vm_reads_actor_before.rs", "vm_reads_registry.rs", "vm_reads_registry_before.rs"]:
            path = root / "crates/deadsync-song-lua/tests/perf" / name
            sources[path.relative_to(root).as_posix()] = sha(path)
    if args.suite == "actor_state":
        for name in ["actor_state.rs", "actor_state_before.rs"]:
            path = root / "crates/deadsync-song-lua/tests/perf" / name
            sources[path.relative_to(root).as_posix()] = sha(path)
    if args.suite == "ignored_args":
        for name in ["ignored_core.rs", "ignored_actor.rs", "ignored_actor_before.rs", "ignored_host.rs", "ignored_host_before.rs", "ignored_tables.rs", "ignored_tables_before.rs"]:
            path = root / "crates/deadsync-song-lua/tests/perf" / name
            sources[path.relative_to(root).as_posix()] = sha(path)
    if args.suite == "prefix_args":
        for name in ["prefix_args.rs", "prefix_args_before.rs"]:
            path = root / "crates/deadsync-song-lua/tests/perf" / name
            sources[path.relative_to(root).as_posix()] = sha(path)
    if args.suite == "method_storage":
        for name in ["method_storage.rs", "method_storage_before.rs", "metatable_storage.rs", "metatable_storage_before.rs"]:
            path = root / "crates/deadsync-song-lua/tests/perf" / name
            sources[path.relative_to(root).as_posix()] = sha(path)
    filters = ["compile::perf::benchmark_compile_finalize"] if args.suite == "finalize" else [
        "actor_classes::native_function_perf::benchmark_native_binding",
        "lua_util::startup_capture_perf::benchmark_snapshot_storage",
    ]
    baseline = "bacdc3e37" if args.suite == "finalize" else "31a7355f5"
    expected_names = 14 if args.suite == "finalize" else 3
    if args.suite == "vm_reads":
        filters = ["actor_classes::vm_reads_perf::benchmark_method_keys", "lua_util::vm_reads_perf::benchmark_registry_and_upvalues"]
        baseline = "4277c1b63"
        expected_names = 6
    if args.suite == "actor_state":
        filters = ["lua_util::actor_state_perf::benchmark_actor_state_paths"]
        baseline = "0908f3cff"
        expected_names = 4
    if args.suite == "ignored_args":
        filters = ["lua_util::ignored_args_perf::benchmark_ignored_actor", "host::ignored_args_perf::benchmark_ignored_host", "tables::ignored_args_perf::benchmark_ignored_tables"]
        baseline = "1c744eebf"
        expected_names = 9
    if args.suite == "prefix_args":
        filters = ["lua_util::prefix_args_perf::benchmark_numeric_factories"]
        baseline = "d95fdfe52"
        expected_names = 12
    if args.suite == "method_storage":
        filters = ["actor_classes::method_storage_perf::benchmark_method_storage", "lua_util::metatable_storage_perf::benchmark_metatable_storage"]
        baseline = "626d9f1f7"
        expected_names = 7
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    records = []
    for index in range(args.rounds):
        env = os.environ.copy()
        env["RUST_MIN_STACK"] = "33554432"
        env.pop("DEADSYNC_PERF_REVERSE", None)
        if index % 2:
            env["DEADSYNC_PERF_REVERSE"] = "1"
        current = []
        for filter_index, test_filter in enumerate(filters):
            run = subprocess.run([str(binary), test_filter,
                                  "--exact", "--ignored", "--nocapture", "--test-threads=1"],
                                 cwd=root, env=env, capture_output=True, text=True, encoding="utf-8")
            stem = "compile-finalize" if args.suite == "finalize" else f"{args.suite}-scope-{filter_index+1}"
            log = root / "target" / f"{stem}-round-{index+1}.log"
            log.write_text(run.stdout + run.stderr, encoding="utf-8")
            assert run.returncode == 0, f"benchmark failed; see {log}"
            for line in run.stdout.splitlines():
                if "DRAW_CAPTURE_METRIC " not in line:
                    continue
                row = json.loads(line.split("DRAW_CAPTURE_METRIC ", 1)[1])
                assert row["baseline_commit"] == baseline
                assert len(row["samples"]) == 9
                row.update(round=index+1, order="after-first" if index % 2 else "before-first")
                current.append(row)
        names = {row["workload"] for row in current}
        assert len(names) == expected_names and len(current) == expected_names * 2
        assert all({row["variant"] for row in current if row["workload"] == name}
                   == {"before", "after"} for name in names)
        records.extend(current)
        print(f"Completed round {index+1}/{args.rounds}", flush=True)
    summary = []
    fields = ("ns", "thread_cycles", "allocations", "reallocations", "allocated_bytes", "frees")
    for name in dict.fromkeys(row["workload"] for row in records):
        values = {}
        for variant in ("before", "after"):
            samples = [sample for row in records if row["workload"] == name
                       and row["variant"] == variant for sample in row["samples"]]
            values[variant] = {key: statistics.median(s[key] / s["units"] for s in samples)
                               for key in fields}
            values[variant]["units_per_second"] = 1e9 / values[variant]["ns"]
        values.update(workload=name,
                      unit=next(row["unit"] for row in records if row["workload"] == name),
                      throughput_gain_percent=100*(values["before"]["ns"]/values["after"]["ns"]-1),
                      cycle_reduction_percent=100*(1-values["after"]["thread_cycles"]/values["before"]["thread_cycles"]))
        summary.append(values)
    assert all(sha(root/name) == digest for name, digest in sources.items()), "source changed while measuring"
    assert sha(binary) == binary_sha, "executable changed while measuring"
    assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip() == head
    report = dict(baseline_commit=baseline, suite=args.suite, worktree_head=head,
                  executable=str(binary), executable_sha256=binary_sha, source_sha256=sources,
                  rustc=subprocess.check_output(["rustc", "+1.98.1", "-Vv"], text=True),
                  system=platform.platform(), processor=platform.processor(), logical_processors=os.cpu_count(),
                  logical_cpu=2, rounds=args.rounds, samples_per_variant_per_workload=args.rounds*9,
                  profile="release opt-level=3, LTO disabled, pinned rustc 1.98.1; same executable and allocator",
                  cycles="QueryThreadCycleTime: scheduled current-thread CPU cycles including kernel execution; not a user-only PMU counter",
                  allocation_scope="current-thread Rust GlobalAlloc, including mlua and its normal Rust-backed Lua allocator; direct native malloc excluded",
                  timing_scope="wrapper orders borrow prepared actors; splits consume prepared compiled DTOs and include output destruction; complete fixtures include context construction, file read, host creation, Lua execution/startup, actor extraction and layer finalization; returned DTO/VM destruction and complete output validation excluded for fixture compiles; inputs and fixtures unchanged",
                  stack_bytes=33554432, records=records, summary=summary)
    if args.suite == "startup":
        report["timing_scope"] = ("Prepared Lua states, keys, functions and actors are excluded. Each sample handles 64 actors with 128 native methods/assignments, or 64 snapshots of a 256-field table. Result handle drops are included; Lua state destruction is excluded. Snapshot GC is stopped before timing. Each of ten samples prepares a new Lua state; first sample discarded.")
    if args.suite == "vm_reads":
        report["timing_scope"] = ("Each sample prepares a fresh Lua state outside timing; first of ten samples discarded. Key cases process 64 actors with 128 bind/assign/lookup operations. Assign/lookup materialize fresh keys from a prepared Lua array inside timing. Registration cases register 128 fresh actors into an initially empty registry, or repeatedly register the last actor in a prepared 1024-entry registry. Upvalue cases query a prepared 32-upvalue function 8192 times. Result/temporary handle destruction is included; VM/input destruction is excluded. GC policy unchanged.")
    if args.suite == "actor_state":
        report["timing_scope"] = ("Each sample prepares a fresh Lua state, a 128-method actor with eight message handlers, deep native fields and wrapper membership, plus 128 non-scalar globals outside timing. Restore cases also prepare 64 scalar snapshots. GC is stopped. Each sample takes 64 actor vector/table snapshots, restores 64 prepared scalar-global snapshots, or scans/collects message-command names 64 times through the actual helper. Selected names move into the current collector; the frozen visitor copies its borrowed name as its original call sites did. Result/temporary drops and consumed scalar snapshot destruction are included; VM/other input destruction is excluded. First of ten samples discarded.")
    if args.suite == "ignored_args":
        report["timing_scope"] = ("Each sample prepares a fresh Lua state and actual actor GetWidth, SCREENMAN GetTopScreen or style GetWidth callback outside timing. A prepared Lua closure calls the callback 8192 times with zero, one receiver, or eight mixed unused inputs. One full warmup loop precedes timing and GC is stopped. Result/argument drops are included; VM/input destruction is excluded. First of ten samples discarded. Frozen functions differ only by module location/visibility; unchanged shared helper calls remain shared.")
    if args.suite == "prefix_args":
        report["timing_scope"] = ("Each sample prepares a fresh Lua state and actual actor wrap-width, size or linear-tween factory outside timing. A prepared Lua closure calls the callback 8192 times with zero, two, three or eight inputs. Tween duration is zero to keep queues bounded. One full warmup loop precedes timing and GC is stopped. Result/argument drops and setter effects are included; VM/input destruction is excluded. First of ten samples discarded. Frozen factories share unchanged downstream helpers.")
    if args.suite == "method_storage":
        report["timing_scope"] = ("Each sample prepares a fresh Lua state outside timing and stops GC. Binding scopes process 64 actors with zero, eight, 128 or 512 shared native methods, including 128 methods with an existing destination table. Native-class scopes install the actual seven-class inventory 16 times. Metatable scopes install 128 actual actor metatables on fresh tables without own methods, sharing the unchanged binder path. Input/result VM destruction is excluded; argument, temporary and Vec destruction is included. First of ten samples discarded. Frozen helper bodies match the baseline.")
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
    for row in summary:
        print(row["workload"], f"throughput {row['throughput_gain_percent']:+.1f}%; cycles {-row['cycle_reduction_percent']:+.1f}%")


if __name__ == "__main__":
    main()
