"""Check immutable evidence and the selected source before committing."""
import csv
import hashlib
import json
from pathlib import Path
import re
import statistics
import subprocess
import tomllib

EVIDENCE = Path(__file__).resolve().parent
ROOT = EVIDENCE.parents[4]
PNG = ROOT / "crates/deadlib-image/png"
BASE = "68a288d39"


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
    checkpoints = {label: record(label) for label in ["baseline", "buffered", "writer", "sub"]}
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
        if label == "sub":
            for path, digest in item["source_sha256"].items():
                assert sha(ROOT / path) == digest
        if label == "baseline":
            for path, digest in item["source_sha256"].items():
                content = subprocess.check_output(["git", "show", BASE + ":" + path], cwd=ROOT)
                if path == "crates/deadlib-image/src/lib.rs":
                    content = content.replace(b"\r\n", b"\n").replace(b"\n", b"\r\n")
                assert hashlib.sha256(content).hexdigest() == digest
    processes = 0
    for name in ["buffered-paired", "writer-paired", "sub-paired", "selected-paired"]:
        comparison = record(name)
        assert comparison["iterations"] == 3
        assert comparison["driver_sha256"] == sha(EVIDENCE.parent / "pixel_pair.py")
        assert len(comparison["measurements"]) == 8
        grouped = {}
        for sample in comparison["measurements"]:
            label = sample["label"]
            assert sample["source_manifest_sha256"] == sha(EVIDENCE / (label + ".json"))
            assert sample["executable_sha256"] == checkpoints[label]["executable_sha256"]
            path = EVIDENCE / sample["path"]
            assert sha(path) == sample["sha256"]
            observations = rows(path)
            assert len(observations) == 124
            for row in observations:
                grouped.setdefault((row["case"], row["operation"]), []).append(row)
            processes += 1
        assert len(grouped) == 124
        for group in grouped.values():
            assert len(group) == 8
            assert len({row["fingerprint"] for row in group}) == 1
        for row in rows(EVIDENCE / (name + "-summary.tsv")):
            assert int(row["after_allocations"]) <= int(row["before_allocations"])
            assert int(row["after_reallocations"]) <= int(row["before_reallocations"])
            assert int(row["after_requested_bytes"]) <= int(row["before_requested_bytes"])
    validation = record("final-validation")
    for path, digest in validation["source_sha256"].items():
        assert sha(PNG / path) == digest
    for test, digest in validation["tests_sha256"].items():
        assert sha(PNG / "tests" / (test + ".rs")) == digest
    for key in ["oracle", "encoder_oracle"]:
        assert sha(Path(validation[key]["path"])) == validation[key]["sha256"]
    commands = validation["commands"] + record("library-validation")["commands"]
    for command in commands:
        assert command["exit_code"] == 0
        assert sha(EVIDENCE / command["log"]) == command["sha256"]
    paired = record("in-process")
    work = Path(paired["executable"]).parent
    assert sha(Path(paired["executable"])) == paired["executable_sha256"]
    assert sha(EVIDENCE / "in-process-build.log") == paired["build_log_sha256"]
    assert sha(EVIDENCE / "compare.rs") == paired["harness_sha256"]
    assert sha(PNG / "examples/support/cpu.rs") == paired["cpu_sha256"]
    assert sha(work / "src/main.rs") == paired["generated_source_sha256"]
    assert sha(ROOT / "crates/deadlib-image/src/lib.rs") == paired["shared_image_sha256"]
    assert paired["profile"] == tomllib.loads((work / "Cargo.toml").read_text())["profile"]["bench"]
    for path, digest in paired["baseline_sha256"].items():
        assert sha(work / "baseline/src" / path.split("/src/", 1)[1]) == digest
        content = subprocess.check_output(["git", "show", BASE + ":" + path], cwd=ROOT)
        assert hashlib.sha256(content).hexdigest() == digest
    for path, digest in paired["selected_sha256"].items():
        assert sha(ROOT / path) == digest
    groups = {}
    pattern = r"^(.+): cycles/op ([\d.]+) -> ([\d.]+);"
    assert len(paired["outputs"]) == 3
    for output in paired["outputs"]:
        path = EVIDENCE / output["path"]
        assert sha(path) == output["sha256"]
        matches = re.findall(pattern, path.read_text(), re.MULTILINE)
        assert len(matches) == 26
        for case, before, after in matches:
            groups.setdefault(case, []).append((float(before), float(after)))
    for row in rows(EVIDENCE / "in-process-summary.tsv"):
        samples = groups[row["case"]]
        assert len(samples) == 3
        assert float(row["before_cycles"]) == statistics.median(a for a, _ in samples)
        assert float(row["after_cycles"]) == statistics.median(b for _, b in samples)
    assert len(groups) == 26
    evidence = {p.name: sha(p) for p in sorted(EVIDENCE.iterdir()) if p.is_file()}
    result = dict(source_base=BASE, selected_checkpoint="sub", pipeline_processes=processes,
                  pipeline_cases=124, in_process_processes=3, in_process_cases=26,
                  validation_commands=len(commands), source_sha256=checkpoints["sub"]["source_sha256"],
                  evidence_sha256=evidence, report_sha256=sha(PNG / "BENCHMARKS.md"))
    destination.write_bytes((json.dumps(result, indent=2) + "\n").encode())
    print("Audit passed: 32 whole-pipeline processes, three in-process comparisons, six validation commands.")


if __name__ == "__main__":
    main()
