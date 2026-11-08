"""Compact validated sessions without retaining overlapping stream prefixes.

C-authored sessions are retained verbatim. Native mixed-session regressions
retain the last image and an independently serialized B.5 snapshot of the raw
DQT/DHT destinations immediately before that image. No entropy is changed.
"""
import argparse
import hashlib
import json
from pathlib import Path
from stream_support import update_tables, prefix_tables

parser = argparse.ArgumentParser()
parser.add_argument("--raw", required=True, type=Path)
parser.add_argument("--out", required=True, type=Path)
parser.add_argument("--reports", required=True, nargs=3, type=Path)
parser.add_argument("--report-out", required=True, type=Path)
args = parser.parse_args()
assert args.raw.resolve() != args.out.resolve()
args.out.mkdir(parents=True, exist_ok=True)
args.report_out.mkdir(parents=True, exist_ok=True)


def sha(data):
    return hashlib.sha256(data).hexdigest()


retained = []
for report_path in args.reports:
    report = json.loads(report_path.read_text())
    part = report["part"]
    rows = []
    for result in report["results"]:
        candidates = [result.pop("fixture")] if part == "c" else result.pop("fixtures")
        for fixture in candidates:
            if fixture is None:
                continue
            streams = []
            for entry in fixture["streams"]:
                data = (args.raw / entry["file"]).read_bytes()
                assert sha(data) == entry["sha256"]
                streams.append(data)
            expected = []
            for image in fixture["images"]:
                dumps = []
                for entry in image:
                    data = (args.raw / entry["file"]).read_bytes()
                    assert sha(data) == entry["sha256"]
                    dumps.append((entry["suffix"], data))
                expected.append(dumps)
            snapshot = part != "c"
            if snapshot:
                bank = {}
                for data in streams[:-1]:
                    update_tables(bank, data)
                streams = [prefix_tables(bank, b"\xff\xd8\xff\xd9"), streams[-1]]
                expected = expected[-1:]
            name = part + "-" + fixture["case"]
            row = {"case": name, "part": part, "new_coverage": fixture["new_coverage"],
                   "table_packet_origin": "independent pre-image raw destination snapshot" if snapshot else "verbatim C jpeg_write_tables",
                   "source_stream_sha256": [s["sha256"] for s in fixture["streams"]], "streams": [], "images": []}
            for i, data in enumerate(streams):
                file = f"{name}.stream-{i}.jpg"
                (args.out / file).write_bytes(data)
                row["streams"].append({"file": file, "sha256": sha(data)})
            for i, image in enumerate(expected):
                dumps = []
                for tail, data in image:
                    file = f"{name}.image-{i}.{tail}"
                    (args.out / file).write_bytes(data)
                    dumps.append({"file": file, "suffix": tail, "sha256": sha(data)})
                row["images"].append(dumps)
            retained.append(row)
            rows.append(name)
    # This counts configured image cases, not distinct output byte strings.
    report["image_cases"] = report.pop("unique_images")
    report["pinned_sessions"] = rows
    report["postprocessor_sha256"] = sha(Path(__file__).read_bytes())
    (args.report_out / f"abbreviated-{part}.json").write_text(json.dumps(report, indent=2) + "\n")
manifest = {"provenance": __doc__, "postprocessor_sha256": sha(Path(__file__).read_bytes()), "sessions": retained}
(args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
print(json.dumps({"sessions": len(retained), "images": sum(len(r["images"]) for r in retained),
                  "files": len(list(args.out.iterdir()))}))
