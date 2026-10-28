"""Audit source, binary, correctness and measurement identities before commit."""
import csv
import hashlib
import json
import math
from pathlib import Path
import re
import statistics
import subprocess
import tomllib

EVIDENCE = Path(__file__).resolve().parent
ROOT = EVIDENCE.parents[4]
PNG = ROOT / "crates/deadlib-image/png"
BASE = "7fdecb9d4"
SELECTED = "header"
CHECKPOINTS = ["baseline", "borrowed", "optional", SELECTED]
COMPARISONS = ["borrowed-paired", "optional-paired", "header-paired", "selected-paired"]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def record(name):
    return json.loads((EVIDENCE / (name + ".json")).read_text())


def rows(path):
    with path.open(newline="") as stream:
        return list(csv.DictReader(stream, delimiter="\t"))


def git_bytes(path):
    return subprocess.check_output(["git", "show", BASE + ":" + path], cwd=ROOT)


def same_number(actual, expected):
    assert math.isclose(float(actual), expected, rel_tol=1e-12), (actual, expected)


def main():
    destination = EVIDENCE / "audit.json"
    assert not destination.exists()
    checkpoints = {label: record(label) for label in CHECKPOINTS}
    for label, item in checkpoints.items():
        assert sha(Path(item["executable"])) == item["executable_sha256"]
        assert sha(EVIDENCE / (label + "-build.log")) == item["build_log_sha256"]
        assert sha(EVIDENCE / (label + ".patch")) == item["patch_sha256"]
        assert sha(ROOT / "Cargo.toml") == item["workspace_manifest_sha256"]
        assert sha(ROOT / "Cargo.lock") == item["lock_sha256"]
        assert sha(EVIDENCE.parent / "pixel_measure.py") == item["driver_sha256"]
        assert item["outputs"] == []
        for path, digest in item["benchmark_sha256"].items():
            assert sha(ROOT / path) == digest
        if label == SELECTED:
            for path, digest in item["source_sha256"].items():
                assert sha(ROOT / path) == digest, path
        if label == "baseline":
            for path, digest in item["source_sha256"].items():
                content = git_bytes(path)
                if path == "crates/deadlib-image/src/lib.rs":
                    content = content.replace(b"\r\n", b"\n").replace(b"\n", b"\r\n")
                assert hashlib.sha256(content).hexdigest() == digest, path
    processes = 0
    for name in COMPARISONS:
        comparison = record(name)
        assert comparison["iterations"] == 3
        assert comparison["driver_sha256"] == sha(EVIDENCE.parent / "pixel_pair.py")
        assert len(comparison["measurements"]) == 8
        grouped = {}
        versions = []
        for sample in comparison["measurements"]:
            label = sample["label"]
            if label not in versions:
                versions.append(label)
            assert sample["source_manifest_sha256"] == sha(EVIDENCE / (label + ".json"))
            assert sample["executable_sha256"] == checkpoints[label]["executable_sha256"]
            path = EVIDENCE / sample["path"]
            assert sha(path) == sample["sha256"]
            observations = rows(path)
            assert len(observations) == 212
            for row in observations:
                grouped.setdefault((row["case"], row["operation"]), {}).setdefault(label, []).append(row)
            processes += 1
        summaries = rows(EVIDENCE / (name + "-summary.tsv"))
        assert len(summaries) == len(grouped) == 212
        for summary in summaries:
            group = grouped[(summary["case"], summary["operation"])]
            assert len({row["fingerprint"] for values in group.values() for row in values}) == 1
            medians = []
            for side, label in zip(["before", "after"], versions):
                values = group[label]
                assert len(values) == 4
                for field in ["fingerprint", "allocations", "reallocations", "requested_bytes"]:
                    assert len({row[field] for row in values}) == 1
                median = statistics.median(float(row["cycles_per_image"]) for row in values)
                medians.append(median)
                same_number(summary[side + "_cycles"], median)
                for field in ["allocations", "reallocations", "requested_bytes"]:
                    assert summary[side + "_" + field] == values[0][field]
            same_number(summary["cpu_throughput"], medians[0] / medians[1])
            for field in ["allocations", "reallocations", "requested_bytes"]:
                assert int(summary["after_" + field]) <= int(summary["before_" + field])
    validation = record("final-validation")
    for path, digest in validation["source_sha256"].items():
        assert sha(PNG / path) == digest
    for test, digest in validation["tests_sha256"].items():
        assert sha(PNG / "tests" / (test + ".rs")) == digest
    assert sha(Path(validation["oracle"]["path"])) == validation["oracle"]["sha256"]
    library = record("library-validation")
    for path, digest in library["source_sha256"].items():
        assert sha(ROOT / path) == digest
    subprocess.run(["git", "diff", "--exit-code", BASE, "--", "crates/deadlib-image/png/tests", "crates/deadlib-image/png/Cargo.toml"], cwd=ROOT, check=True, capture_output=True)
    commands = validation["commands"] + library["commands"]
    for command in commands:
        assert command["exit_code"] == 0
        assert sha(EVIDENCE / command["log"]) == command["sha256"]
    callers = [("in-process", "baseline", "compare.py", "compare.rs", 212)]
    for label, before_checkpoint, driver, harness, expected_cases in callers:
        checkpoint = SELECTED
        paired = record(label)
        work = Path(paired["executable"]).parent
        assert sha(Path(paired["executable"])) == paired["executable_sha256"]
        assert sha(EVIDENCE / (label + "-build.log")) == paired["build_log_sha256"]
        assert sha(EVIDENCE / harness) == paired["harness_sha256"]
        if "driver_sha256" in paired:
            assert sha(EVIDENCE / driver) == paired["driver_sha256"]
        assert sha(PNG / "examples/support/cpu.rs") == paired["cpu_sha256"]
        assert sha(PNG / "tests/support/chunks.rs") == paired["chunks_sha256"]
        assert sha(work / "src/chunks.rs") == paired["chunks_sha256"]
        assert sha(work / "src/main.rs") == paired["generated_source_sha256"]
        assert sha(ROOT / "crates/deadlib-image/src/lib.rs") == paired["shared_image_sha256"]
        assert paired["profile"] == tomllib.loads((work / "Cargo.toml").read_text())["profile"]["bench"]
        expected = {path: digest for path, digest in checkpoints[checkpoint]["source_sha256"].items()
                    if path.startswith("crates/deadlib-image/png/")}
        assert paired["selected_sha256"] == expected
        for path, digest in paired["baseline_sha256"].items():
            assert sha(work / "baseline/src" / path.split("/src/", 1)[1]) == digest
            if before_checkpoint == "baseline":
                assert hashlib.sha256(git_bytes(path)).hexdigest() == digest
            else:
                assert checkpoints[before_checkpoint]["source_sha256"][path] == digest
        groups = {}
        if before_checkpoint != "baseline":
            assert paired["before_checkpoint"] == before_checkpoint
            assert paired["before_manifest_sha256"] == sha(EVIDENCE / (before_checkpoint + ".json"))
        assert len(paired["outputs"]) == 3
        size_probes = set()
        for output in paired["outputs"]:
            path = EVIDENCE / output["path"]
            assert sha(path) == output["sha256"]
            sizes = re.findall(r"^type-sizes inflater (\d+) -> (\d+); stream-decoder (\d+) -> (\d+)$", path.read_text(), re.MULTILINE)
            owned = re.findall(r"^inflater-owned-zero (\d+) -> (\d+)$", path.read_text(), re.MULTILINE)
            assert len(sizes) == len(owned) == 1
            size_probes.add(tuple(map(int, sizes[0] + owned[0])))
            matches = re.findall(r"^(.+): cycles/op ([\d.]+) -> ([\d.]+);", path.read_text(), re.MULTILINE)
            assert len(matches) == expected_cases
            for case, before, after in matches:
                groups.setdefault(case, []).append((float(before), float(after)))
        assert len(size_probes) == 1
        object_sizes = dict(zip(["inflater_before", "inflater_after", "stream_before", "stream_after", "zero_owned_before", "zero_owned_after"], size_probes.pop()))
        summaries = rows(EVIDENCE / (label + "-summary.tsv"))
        assert len(summaries) == len(groups) == expected_cases
        for row in summaries:
            samples = groups[row["case"]]
            assert len(samples) == 3
            same_number(row["before_cycles"], statistics.median(a for a, _ in samples))
            same_number(row["after_cycles"], statistics.median(b for _, b in samples))
            same_number(row["cpu_throughput"], float(row["before_cycles"]) / float(row["after_cycles"]))
    result = dict(source_base=BASE, selected_checkpoint=SELECTED, pipeline_processes=processes,
                  pipeline_cases=212, in_process_processes=3 * len(callers), in_process_cases={label: count for label, _, _, _, count in callers},
                  validation_commands=len(commands), source_sha256=checkpoints[SELECTED]["source_sha256"],
                  object_sizes=object_sizes,
                  test_support_sha256={p.relative_to(ROOT).as_posix(): sha(p) for p in sorted((PNG / "tests/support").rglob("*.rs"))},
                  evidence_sha256={p.name: sha(p) for p in sorted(EVIDENCE.iterdir()) if p.is_file()},
                  report_sha256=hashlib.sha256((PNG / "BENCHMARKS.md").read_bytes().replace(b"\r\n", b"\n")).hexdigest())
    destination.write_bytes((json.dumps(result, indent=2) + "\n").encode())
    print(f"Audit passed: {processes} pipeline processes, {3 * len(callers)} paired callers, six validation commands.")


if __name__ == "__main__":
    main()
