"""Add the native baseline DNL regression already verified by the C gate."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("--report", required=True, type=Path)
parser.add_argument("--native", required=True, type=Path)
parser.add_argument("--out", required=True, type=Path)
args = parser.parse_args()
report = json.loads(args.report.read_text())
assert hashlib.sha256(args.native.read_bytes()).hexdigest() == report["tools"]["native"]
record = next(r for r in report["results"] if r["markers"] == [192] and r["initial_height"] == "0"
              and r["original_header_reference_exact"] and "-r0-" in r["case"])
source_name = record["case"].rsplit("-h0-", 1)[0].removeprefix("sequential-")
source = Path(__file__).parent / "fixtures/sequential" / (source_name + ".jpg")
assert hashlib.sha256(source.read_bytes()).hexdigest() == record["source_sha256"]
image = args.out / "native-baseline-p8.jpg"
result = subprocess.run([str(args.native), "dct", str(source), str(image), "0", "0", "0", "0"], capture_output=True)
assert result.returncode == 0 and not result.stderr
assert hashlib.sha256(image.read_bytes()).hexdigest() == record["sha256"]
assert hashlib.sha256(image.with_suffix(".frame-0.coef").read_bytes()).hexdigest() == record["frame_data_sha256"]
shutil.move(image.with_suffix(".expected.samples"), image.with_suffix(".samples"))
manifest_path = args.out / "manifest.json"
manifest = json.loads(manifest_path.read_text())
manifest["provenance"] = "Independent C-authored DNL JPEG sources plus one native-authored baseline DNL file checked directly by C. C-source repairs remove only empty DAC/EXP00; DNL and unknown heights remain. C original-header checks are distinguished from known-header entropy checks. Not a standards conformance corpus."
manifest["cases"] = [r for r in manifest["cases"] if r["fixture"] != "native-baseline-p8"]
manifest["cases"].append(record | {"fixture": "native-baseline-p8", "source_markers": [192], "native_authored": True})
manifest["files"] = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(args.out.iterdir()) if p.suffix in (".jpg", ".coef", ".samples")}
manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
print("Pinned the C-verified native baseline DNL case.")
