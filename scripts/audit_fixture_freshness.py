"""Inventory actual oracle captures separately from requests and metadata.

This is an audit, not a parity result: a current version or receipt still needs
its numerical comparisons to pass. It never runs or needs an external harness.
"""
import argparse
from collections import Counter
import io
import json
from pathlib import Path

import zstandard

from fixture_capture_hash import digest


def load(path):
    raw = path.read_bytes()
    if path.suffix == ".zst":
        with zstandard.ZstdDecompressor().stream_reader(io.BytesIO(raw)) as stream:
            raw = stream.read()
    return json.loads(raw)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    contract = load(root / "tests/fixtures/oracle-contract.json")
    receipts = {}
    groups = list(contract["groups"])
    if "native_gameplay_controls" in contract:
        groups.append(contract["native_gameplay_controls"])
    if "native_notefield_controls" in contract:
        groups.append(contract["native_notefield_controls"])
    for group in groups:
        receipt_path = root / group["captures"]
        if not receipt_path.is_file():
            continue
        receipt = load(receipt_path)
        for entry in receipt["fixtures"]:
            receipts[entry["output"]] = (receipt, entry, group)
    captures = []
    for directory in ("itgmania-actors", "itgmania-song-lua-micro", "itgmania-primary-bitmaps"):
        for path in sorted((root / "tests/fixtures" / directory).rglob("*")):
            if not (path.name.endswith(".json") or path.name.endswith(".json.zst")):
                continue
            capture = load(path)
            relative = path.relative_to(root).as_posix()
            receipt, entry, group = receipts.get(relative, ({}, {}, {}))
            declared = capture.get("harness_version") if isinstance(capture, dict) else None
            oracle = str(capture.get("oracle", "")) if isinstance(capture, dict) else entry.get("oracle", "")
            actor_samples = isinstance(capture, dict) and any(
                isinstance(sample, dict) and "actors" in sample
                for sample in capture.get("samples", [])[:3]
            ) if isinstance(capture, dict) and isinstance(capture.get("samples"), list) else False
            if isinstance(capture, list) and entry.get("capture_kind") == "rejections":
                kind = "native-rejection"
            elif not isinstance(capture, dict):
                continue
            elif directory == "itgmania-primary-bitmaps" and path.name == "capture.json":
                kind = "native-bitmap"
            elif oracle == "itgmania_notefield_render_trace":
                kind = "native-notefield"
            elif "end_position" in capture and isinstance(capture.get("update_frames"), list):
                kind = "song-trace"
            elif actor_samples or (oracle.startswith("itgmania_native_")
                    and isinstance(capture.get("provenance"), dict)
                    and any(isinstance(capture.get(key), (list, dict))
                            for key in ("samples", "cases", "sprites"))):
                kind = "native-actor"
            elif "native ITGmania ScreenGameplay" in oracle and isinstance(capture.get("frames"), list):
                kind = "native-gameplay"
            else:
                continue
            status = "unversioned"
            if declared:
                status = "declared-current" if declared == contract["harness_version"] else "older-version"
            if kind == "native-bitmap":
                current = (capture["harness_revision"] == contract["harness_revision"]
                           and declared == contract["harness_version"] + "+primary-codecs"
                           and capture["primary_codec_revision"] == contract["primary_itgmania_revision"])
                status = "current-primary-codec-receipt" if current else "older-primary-codec-receipt"
                if digest(path) != contract["primary_bitmap_capture_sha256"]:
                    status = "changed-after-capture"
            if receipt:
                current = (receipt.get("harness_revision") == contract["harness_revision"]
                           and receipt.get("harness_version") == group.get("harness_version", contract["harness_version"]))
                status = ("current-adapter-receipt" if group.get("driver") else "current-receipt") if current else "older-receipt"
                if group.get("driver") and receipt.get("review_status") != "accepted":
                    status = "unreviewed-adapter-receipt"
                if kind in ["native-gameplay", "native-notefield"]:
                    current = current and receipt.get("native_revision") == contract["primary_itgmania_revision"]
                    status = "current-primary-receipt" if current else "older-primary-receipt"
                    if receipt.get("review_status") != "accepted":
                        status = "current-capture-long-replay-deferred" if current and receipt.get("review_status") == "captured-long-replay-deferred" else "unreviewed-primary-receipt"
                if digest(path) != entry["output_sha256"]:
                    status = "changed-after-capture"
            historical = next((item for item in contract.get("historical_captures", []) if item["path"] == relative), None)
            if historical:
                status = "historical-superseded" if digest(path) == historical["sha256"] else "changed-historical-capture"
            captures.append({"path": relative, "kind": kind, "oracle": oracle,
                             "declared_version": declared,
                             "receipt_version": receipt.get("harness_version"),
                             "status": status})
    catalog = load(root / "tests/fixtures/full_song_lua/committed-songs.json")
    summary = {
        kind: {"count": sum(item["kind"] == kind for item in captures),
               "status": dict(Counter(item["status"] for item in captures if item["kind"] == kind))}
        for kind in ("song-trace", "native-actor", "native-gameplay", "native-notefield", "native-rejection", "native-bitmap")
    }
    result = {"harness_version": contract["harness_version"],
              "harness_revision": contract["harness_revision"],
              "summary": summary,
              "retained_song_archives": {"count": len(catalog),
                  "versions": dict(sorted(Counter(item["harness_version"] for item in catalog).items()))},
              "captures": captures}
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
