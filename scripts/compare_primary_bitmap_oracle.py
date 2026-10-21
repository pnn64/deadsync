"""Compare committed bitmap controls with an explicitly recorded codec adapter.

Outputs are diagnostic captures. This does not replace fixtures or promote the
codec adapter as evidence for unrelated engine behavior.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import subprocess


def load(path):
    return json.loads(path.read_bytes())


def compare(old, new, counts, examples, path=""):
    if type(old) != type(new):
        category = "shape"
    elif isinstance(old, dict):
        for key in old.keys() | new.keys():
            if key not in old or key not in new:
                counts["shape"] += 1
            else:
                compare(old[key], new[key], counts, examples, path + "." + key)
        return
    elif isinstance(old, list):
        if len(old) != len(new):
            counts["shape"] += 1
        for index, (a, b) in enumerate(zip(old, new)):
            compare(a, b, counts, examples, path + f"[{index}]")
        return
    elif old == new:
        return
    else:
        category = "upload_pixels" if ".upload.pixels[" in path else (
            "decoded_pixels" if ".loaded.pixels[" in path else "metadata")
    counts[category] += 1
    if len(examples) < 12:
        examples.append({"path": path, "old": old, "primary": new})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adapter", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    adapter = load(args.adapter.resolve())
    assert adapter["build_exit_code"] == 0
    assert len(adapter["codec_support_proofs"]) >= 17
    binary = Path(adapter["binary"])
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == adapter["binary_sha256"]
    version = subprocess.check_output([str(binary), "--version"], text=True).strip()
    assert version.endswith("+primary-codecs"), version
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    rows = []
    for folder in ["bitmap-file-decoding", "indexed-bitmap-files", "texture-image-getters",
                   "texture-sprite-preparation"]:
        source = root / "tests/fixtures/itgmania-song-lua-micro" / folder
        control = load(source / "control.json")
        baseline = {case["name"]: case for case in load(source / "native.json")["cases"]}
        target = output / folder
        target.mkdir()
        for index, spec in enumerate(control["texture_files"]):
            name = spec["name"]
            resource = (source / spec["file"]).resolve()
            resource.relative_to(root)
            request = {"name": control["name"], "texture_files": [
                {**spec, "file": str(resource)}]}
            request_path = target / f"{index}.request.json"
            result_path = target / f"{index}.native.json"
            request_path.write_bytes(json.dumps(request).encode("utf-8"))
            result = subprocess.run([str(binary), "actor-conformance", str(request_path),
                                     "--out", str(result_path)], cwd=root,
                                    capture_output=True, encoding="utf-8")
            old = baseline.get(name)
            row = {"folder": folder, "name": name, "baseline_accepted": old is not None,
                   "primary_accepted": result.returncode == 0, "error": result.stderr.strip(),
                   "source": resource.relative_to(root).as_posix(),
                   "source_sha256": hashlib.sha256(resource.read_bytes()).hexdigest()}
            if result.returncode == 0:
                document = load(result_path)
                assert document["provenance"]["git_revision"] == adapter["native_baseline_revision"]
                assert document["provenance"]["git_dirty"] == "false"
                if old is not None:
                    counts, examples = Counter(), []
                    compare(old, document["cases"][0], counts, examples)
                    row["differences"] = dict(counts)
                    row["examples"] = examples
            if row["baseline_accepted"] != row["primary_accepted"] or row.get("differences"):
                print(folder, name, "accepted", row["baseline_accepted"], "->",
                      row["primary_accepted"], row.get("differences", {}), row["error"], flush=True)
            rows.append(row)
    summary = {"cases": len(rows), "acceptance_changes": sum(
        row["baseline_accepted"] != row["primary_accepted"] for row in rows),
        "changed_uploads": sum(bool(row.get("differences", {}).get("upload_pixels")) for row in rows),
        "changed_decodes": sum(bool(row.get("differences", {}).get("decoded_pixels")) for row in rows)}
    record = {"adapter": str(args.adapter.resolve()), "adapter_binary_sha256": adapter["binary_sha256"],
              "primary_codec_revision": adapter["itgmania_revision"], "scope": adapter["scope"],
              "summary": summary, "cases": rows}
    (output / "comparison.json").write_bytes((json.dumps(record, indent=2) + "\n").encode())
    print(json.dumps(summary), flush=True)


if __name__ == "__main__":
    main()
