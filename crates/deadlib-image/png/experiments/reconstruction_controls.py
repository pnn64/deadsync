"""Rerun saved checkpoints in reverse order and summarize all six processes."""
import csv
import hashlib
import json
from pathlib import Path
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[4]
PNG = ROOT / "crates/deadlib-image/png"
EVIDENCE = PNG / "experiments/reconstruction-20261028"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    controls = []
    for label in ["first-row", "packed", "scale", "baseline"]:
        source = EVIDENCE / f"{label}.json"
        record = json.loads(source.read_text())
        binary = Path(record["executable"])
        assert sha(binary) == record["executable_sha256"]
        for run in range(1, 4):
            path = EVIDENCE / f"{label}-control{run}.tsv"
            assert not path.exists()
            result = subprocess.run([str(binary)], cwd=ROOT, capture_output=True)
            assert result.returncode == 0, result.stderr.decode(errors="replace")
            path.write_bytes(result.stdout.replace(b"\r\n", b"\n"))
            controls.append(dict(path=path.name, sha256=sha(path), source=source.name,
                                 source_sha256=sha(source), executable_sha256=sha(binary)))
            (EVIDENCE / "controls.json").write_text(json.dumps(controls, indent=2) + "\n")
            print(f"{label} control {run} complete", flush=True)
    checkpoints = {}
    fingerprints = {}
    for label in ["baseline", "scale", "packed", "first-row"]:
        cases = {}
        for kind in ["run", "control"]:
            for run in range(1, 4):
                with (EVIDENCE / f"{label}-{kind}{run}.tsv").open() as stream:
                    rows = list(csv.DictReader(stream, delimiter="\t"))
                assert len(rows) == 26
                for row in rows:
                    key = row["case"] + "/" + row["operation"]
                    assert fingerprints.setdefault(key, row["fingerprint"]) == row["fingerprint"]
                    cases.setdefault(key, []).append(row)
        checkpoints[label] = {}
        for key, rows in cases.items():
            values = {field: [float(row[field]) for row in rows] for field in
                      ["cycles_per_image", "ns_per_image", "MiB_per_second", "allocations", "reallocations", "requested_bytes"]}
            for field in ["allocations", "reallocations", "requested_bytes"]:
                assert len(set(values[field])) == 1
            checkpoints[label][key] = dict(median={k: statistics.median(v) for k, v in values.items()},
                                           cycles_range=[min(values["cycles_per_image"]), max(values["cycles_per_image"])],
                                           fingerprint=fingerprints[key], processes=len(rows))
    summary = dict(checkpoints=checkpoints, comparisons={})
    for before, after, keys in [
        ("baseline", "scale", [key for key in fingerprints if not key.endswith("/decode")]),
        ("scale", "packed", [f"gray{d}-large/decode" for d in [1, 2, 4]]),
        ("packed", "first-row", [key for key in fingerprints if key.endswith("/decode")]),
    ]:
        comparison = summary["comparisons"][before + "-to-" + after] = {}
        for key in keys:
            old = checkpoints[before][key]["median"]
            new = checkpoints[after][key]["median"]
            comparison[key] = dict(before=old, after=new, cpu_throughput_ratio=old["cycles_per_image"] / new["cycles_per_image"])
    (EVIDENCE / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print("All fingerprints and allocation counts agree; summary written.", flush=True)


if __name__ == "__main__":
    main()
