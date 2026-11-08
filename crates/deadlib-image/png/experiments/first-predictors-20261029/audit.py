"""Check immutable evidence and the selected source before committing."""
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
BASE = "8314615bc"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def record(name):
    return json.loads((EVIDENCE / (name + ".json")).read_text())


def rows(path):
    with path.open(newline="") as stream:
        return list(csv.DictReader(stream, delimiter="\t"))


def main():
    destination = EVIDENCE / "audit.json"
    assert not destination.exists()
    checkpoints = {label: record(label) for label in ["baseline", "buffered", "writer", "adaptive"]}
    for label, item in checkpoints.items():
        assert sha(Path(item["executable"])) == item["executable_sha256"]
        assert sha(EVIDENCE / (label + "-build.log")) == item["build_log_sha256"]
        assert sha(EVIDENCE / (label + ".patch")) == item["patch_sha256"]
        assert sha(ROOT / "Cargo.toml") == item["workspace_manifest_sha256"]
        assert sha(ROOT / "Cargo.lock") == item["lock_sha256"]
        assert sha(EVIDENCE / "measure.py") == item["driver_sha256"]
        assert item["outputs"] == []
        messages = [json.loads(line) for line in (EVIDENCE / (label + "-build.log")).read_bytes().splitlines()
                    if line.startswith(b"{")]
        libs = [m for m in messages if m.get("reason") == "compiler-artifact"
                and m["target"]["name"] == "deadlib_image_png"]
        assert len(libs) == 1 and not libs[0]["fresh"]
        for path, digest in item["benchmark_sha256"].items():
            assert sha(ROOT / path) == digest
        if label == "adaptive":
            for path, digest in item["source_sha256"].items():
                assert sha(ROOT / path) == digest
        if label == "baseline":
            for path, digest in item["source_sha256"].items():
                content = subprocess.check_output(["git", "show", BASE + ":" + path], cwd=ROOT)
                if path == "crates/deadlib-image/src/lib.rs":
                    content = content.replace(b"\r\n", b"\n").replace(b"\n", b"\r\n")
                assert hashlib.sha256(content).hexdigest() == digest
    processes = 0
    for name in ["buffered-paired", "writer-paired", "adaptive-paired", "selected-paired"]:
        comparison = record(name)
        assert comparison["iterations"] == 3
        assert comparison["driver_sha256"] == sha(EVIDENCE / "pair.py")
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
            assert len(observations) == 228
            for row in observations:
                grouped.setdefault((row["case"], row["operation"]), {}).setdefault(label, []).append(row)
            processes += 1
        summaries = rows(EVIDENCE / (name + "-summary.tsv"))
        assert len(grouped) == len(summaries) == 228
        for summary in summaries:
            group = grouped[(summary["case"], summary["operation"])]
            assert len({r["fingerprint"] for rs in group.values() for r in rs}) == 1
            medians = []
            for side, label in zip(["before", "after"], versions):
                values = group[label]
                assert len(values) == 4
                for field in ["fingerprint", "allocations", "reallocations", "requested_bytes"]:
                    assert len({r[field] for r in values}) == 1
                median = statistics.median(float(r["cycles_per_image"]) for r in values)
                medians.append(median)
                assert math.isclose(float(summary[side + "_cycles"]), median, rel_tol=1e-12)
                for field in ["allocations", "reallocations", "requested_bytes"]:
                    assert summary[side + "_" + field] == values[0][field]
            assert math.isclose(float(summary["cpu_throughput"]), medians[0] / medians[1], rel_tol=1e-12)
            for field in ["allocations", "reallocations", "requested_bytes"]:
                assert int(summary["after_" + field]) <= int(summary["before_" + field])
    validation = record("final-validation")
    for path, digest in validation["source_sha256"].items():
        assert sha(PNG / path) == digest
    for test, digest in validation["tests_sha256"].items():
        assert sha(PNG / "tests" / (test + ".rs")) == digest
    for key in ["oracle", "encoder_oracle"]:
        assert sha(Path(validation[key]["path"])) == validation[key]["sha256"]
    library = record("library-validation")
    for path, digest in library["source_sha256"].items():
        assert sha(ROOT / path) == digest
    commands = validation["commands"] + library["commands"]
    for command in commands:
        assert command["exit_code"] == 0
        assert sha(EVIDENCE / command["log"]) == command["sha256"]
    paired = record("in-process")
    work = Path(paired["executable"]).parent
    assert sha(Path(paired["executable"])) == paired["executable_sha256"]
    assert sha(EVIDENCE / "in-process-caller-build.log") == paired["build_log_sha256"]
    assert sha(EVIDENCE / "compare.rs") == paired["harness_sha256"]
    assert sha(PNG / "examples/support/cpu.rs") == paired["cpu_sha256"]
    assert sha(work / "src/main.rs") == paired["generated_source_sha256"]
    assert sha(ROOT / "crates/deadlib-image/src/lib.rs") == paired["shared_image_sha256"]
    assert paired["profile"] == tomllib.loads((work / "Cargo.toml").read_text())["profile"]["bench"]
    assert sha(EVIDENCE / "compare.py") == paired["driver_sha256"]
    assert paired["selected_sha256"] == {p: h for p, h in checkpoints["adaptive"]["source_sha256"].items()
                                           if p.startswith("crates/deadlib-image/png/")}
    for path, digest in paired["baseline_sha256"].items():
        assert sha(work / "baseline/src" / path.split("/src/", 1)[1]) == digest
        content = subprocess.check_output(["git", "show", BASE + ":" + path], cwd=ROOT)
        assert hashlib.sha256(content).hexdigest() == digest
    for path, digest in paired["selected_sha256"].items():
        assert sha(ROOT / path) == digest
    groups = {}
    memory_probes = {}
    pattern = r"^(.+): cycles/op ([\d.]+) -> ([\d.]+);"
    assert len(paired["outputs"]) == 3
    for output in paired["outputs"]:
        path = EVIDENCE / output["path"]
        assert sha(path) == output["sha256"]
        memory = re.findall(r"^(.+): writer-owned (\d+) -> (\d+)$", path.read_text(encoding="utf-8"), re.MULTILINE)
        assert len(memory) == 50
        for case, before, after in memory:
            memory_probes.setdefault(case, []).append((int(before), int(after)))
        matches = re.findall(pattern, path.read_text(encoding="utf-8"), re.MULTILINE)
        assert len(matches) == 100
        for case, before, after in matches:
            groups.setdefault(case, []).append((float(before), float(after)))
    for row in rows(EVIDENCE / "in-process-summary.tsv"):
        samples = groups[row["case"]]
        assert len(samples) == 3
        assert float(row["before_cycles"]) == statistics.median(a for a, _ in samples)
        assert float(row["after_cycles"]) == statistics.median(b for _, b in samples)
    assert len(groups) == 100
    assert len(memory_probes) == 50
    for case, values in memory_probes.items():
        assert len(values) == 3 and len(set(values)) == 1
        assert values[0][1] <= values[0][0], case
    report = (PNG / "BENCHMARKS.md").read_bytes().replace(b"\r\n", b"\n")
    historical = subprocess.check_output(["git", "show", BASE + ":crates/deadlib-image/png/BENCHMARKS.md"], cwd=ROOT)
    assert report.startswith(historical.rstrip() + b"\n\n")
    subprocess.run(["git", "diff", "--exit-code", BASE, "--", "crates/deadlib-image/png/tests", "crates/deadlib-image/png/Cargo.toml"], cwd=ROOT, check=True, capture_output=True)
    evidence = {p.name: sha(p) for p in sorted(EVIDENCE.iterdir()) if p.is_file()}
    result = dict(source_base=BASE, selected_checkpoint="adaptive", pipeline_processes=processes,
                  pipeline_cases=228, in_process_processes=3, in_process_cases=100,
                  writer_memory_probes={case: dict(before=values[0][0], after=values[0][1]) for case, values in memory_probes.items()},
                  validation_commands=len(commands), source_sha256=checkpoints["adaptive"]["source_sha256"],
                  evidence_sha256=evidence, report_sha256=hashlib.sha256(report).hexdigest(),
                  test_support_sha256={p.relative_to(ROOT).as_posix(): sha(p) for p in sorted((PNG / "tests/support").rglob("*.rs"))})
    destination.write_bytes((json.dumps(result, indent=2) + "\n").encode())
    print("Audit passed: 32 whole-pipeline processes, three in-process comparisons, six validation commands.")


if __name__ == "__main__":
    main()
