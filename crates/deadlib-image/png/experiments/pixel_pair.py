"""Alternate immutable palette/opaque binaries to investigate shared-host drift."""
import csv
import io
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys

from pixel_measure import EVIDENCE, ROOT, save, sha


def main():
    before_label, selected_label, prefix = (
        sys.argv[1:4] if len(sys.argv) == 4 else
        ("palette", "inline", "selected-paired") if len(sys.argv) > 1 else
        ("palette", "opaque", "paired")
    )
    assert all(value.replace("-", "").isalnum() for value in [before_label, selected_label, prefix])
    manifest = EVIDENCE / f"{prefix}.json"
    assert not manifest.exists()
    records = {label: json.loads((EVIDENCE / f"{label}.json").read_text()) for label in [before_label, selected_label]}
    evidence = dict(driver_sha256=sha(Path(__file__)), measurements=[])
    groups = {label: {} for label in records}
    env = dict(os.environ, DEADLIB_PNG_BENCH_ITERS="12")
    for round in range(4):
        order = [before_label, selected_label] if round % 2 == 0 else [selected_label, before_label]
        for label in order:
            record = records[label]
            binary = Path(record["executable"])
            assert sha(binary) == record["executable_sha256"]
            path = EVIDENCE / f"{prefix}-{round + 1}-{label}.tsv"
            assert not path.exists()
            result = subprocess.run([str(binary)], cwd=ROOT, env=env, capture_output=True)
            assert result.returncode == 0, result.stderr.decode(errors="replace")
            path.write_bytes(result.stdout.replace(b"\r\n", b"\n"))
            evidence["measurements"].append(dict(round=round + 1, label=label, path=path.name, sha256=sha(path),
                                                 source_manifest_sha256=sha(EVIDENCE / f"{label}.json"), executable_sha256=sha(binary)))
            save(manifest, evidence)
            for row in csv.DictReader(io.StringIO(path.read_text()), delimiter="\t"):
                groups[label].setdefault((row["case"], row["operation"]), []).append(row)
            print(f"paired round {round + 1} {label} passed", flush=True)
    rows = []
    for key, before_rows in groups[before_label].items():
        after_rows = groups[selected_label][key]
        assert {row["fingerprint"] for row in before_rows + after_rows} == {before_rows[0]["fingerprint"]}
        before = statistics.median(float(row["cycles_per_image"]) for row in before_rows)
        after = statistics.median(float(row["cycles_per_image"]) for row in after_rows)
        rows.append(dict(case=key[0], operation=key[1], before_cycles=before, after_cycles=after,
                         cpu_throughput=before / after,
                         before_MiB=statistics.median(float(row["MiB_per_second"]) for row in before_rows),
                         after_MiB=statistics.median(float(row["MiB_per_second"]) for row in after_rows)))
    stream = io.StringIO(newline="")
    writer = csv.DictWriter(stream, fieldnames=list(rows[0]), delimiter="\t", lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    (EVIDENCE / f"{prefix}-summary.tsv").write_bytes(stream.getvalue().encode())


if __name__ == "__main__":
    main()
