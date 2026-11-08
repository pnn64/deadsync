"""Recompute the supplemental paired-caller summaries without replacing evidence."""
import csv
import io
import json
from pathlib import Path
import re
import statistics

EVIDENCE = Path(__file__).resolve().parent


def main():
    manifests = [(p, 212) for p in sorted(EVIDENCE.glob("in-process*.json"))]
    manifests += [(EVIDENCE / name, count) for name, count in [("setup.json", 15), ("replication.json", 15), ("repeat-focus.json", 9), ("repeat-clean-focus.json", 9), ("fixed-focus.json", 6)]
                  if (EVIDENCE / name).exists()]
    for manifest, expected in manifests:
        groups = {}
        record = json.loads(manifest.read_text())
        assert len(record["outputs"]) == 3
        for output in record["outputs"]:
            matches = re.findall(r"^(.+): cycles/op ([\d.]+) -> ([\d.]+);",
                                 (EVIDENCE / output["path"]).read_text(), re.MULTILINE)
            assert len(matches) == expected
            for case, before, after in matches:
                groups.setdefault(case, []).append((float(before), float(after)))
        assert len(groups) == expected
        rows = []
        for case, values in groups.items():
            assert len(values) == 3
            before = statistics.median(a for a, _ in values)
            after = statistics.median(b for _, b in values)
            rows.append(dict(case=case, before_cycles=before, after_cycles=after,
                             cpu_throughput=before / after,
                             min_before=min(a for a, _ in values), max_before=max(a for a, _ in values),
                             min_after=min(b for _, b in values), max_after=max(b for _, b in values)))
        stream = io.StringIO(newline="")
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]), delimiter="\t", lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
        content = stream.getvalue().encode()
        path = manifest.with_name(manifest.stem + "-summary.tsv")
        if path.exists():
            assert path.read_bytes() == content
        else:
            path.write_bytes(content)
        print(f"Verified {path.name}: {len(rows)} complete API comparisons")


if __name__ == "__main__":
    main()
