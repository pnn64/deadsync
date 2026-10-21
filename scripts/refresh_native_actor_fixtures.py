"""Regenerate bundled actor controls with an explicitly selected native harness.

Run from any directory. Generation uses repository inputs; ordinary tests need
only the committed captures, not a harness checkout or executable.
"""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import time

import zstandard

from fixture_capture_hash import HASH_ENCODING, digest


def capture_rejections(executable, request, document, root, provenance):
    cases = []
    for spec in document["texture_files"]:
        resource = (request.parent / spec["file"]).resolve()
        resource.relative_to(root)
        native_request = {"name": document["name"], "texture_files": [
            {**spec, "file": str(resource)}]}
        with tempfile.TemporaryDirectory(prefix="actor-rejection-") as temporary:
            input_path = Path(temporary) / "request.json"
            output_path = Path(temporary) / "native.json"
            input_path.write_bytes(json.dumps(native_request).encode("utf-8"))
            result = subprocess.run([str(executable), "actor-conformance", str(input_path),
                                     "--out", str(output_path)], cwd=root,
                                    capture_output=True, encoding="utf-8")
        assert result.returncode == 1, (spec["name"], result.returncode, result.stderr)
        assert result.stderr.strip() == document["expected_error"], result.stderr
        cases.append({**spec, "exit_code": result.returncode,
                      "error": result.stderr.strip(), "source_sha256": digest(resource)})
    raw = (json.dumps(cases, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    return raw, {"oracle": "itgmania_native_actor_rejection", "provenance": provenance}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--harness", required=True, type=Path)
    parser.add_argument("--harness-source", required=True, type=Path)
    parser.add_argument("--manifest", type=Path,
                        default=Path("tests/fixtures/itgmania-actors/inputs/manifest.json"))
    parser.add_argument("--output-root", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    executable = args.harness.resolve()
    source = args.harness_source.resolve()
    manifest_path = root / args.manifest
    manifest = json.loads(manifest_path.read_bytes())
    version = subprocess.check_output([str(executable), "--version"],
                                      text=True, encoding="utf-8").strip()
    assert version == manifest["harness_version"], (version, manifest["harness_version"])
    revision = subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"],
                                       text=True, encoding="utf-8").strip()
    assert revision == manifest["harness_revision"], (revision, manifest["harness_revision"])
    assert not subprocess.check_output(
        ["git", "-C", str(source), "status", "--porcelain", "--untracked-files=no"],
        text=True, encoding="utf-8").strip(), "harness tracked sources are modified"
    executable_hash = digest(executable)
    receipt = {"schema_version": 1, "hash_encoding": HASH_ENCODING, "harness_version": version,
               "harness_revision": revision, "harness_binary_sha256": executable_hash,
               "request_manifest_sha256": digest(manifest_path), "fixtures": []}
    output_root = args.output_root.resolve()
    for entry in manifest["fixtures"]:
        request = (root / entry["input"]).resolve()
        request.relative_to(root)
        relative = Path(entry["output"])
        assert not relative.is_absolute() and ".." not in relative.parts
        output = output_root / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        started = time.monotonic()
        request_document = json.loads(request.read_bytes())
        if entry.get("capture_kind") == "rejections":
            # A successful native capture from this same unchanged binary pins
            # the embedded source identity for its stderr-only rejection runs.
            assert receipt["fixtures"], "capture a positive native control first"
            provenance = receipt["fixtures"][0]["native_provenance"]
            raw, result = capture_rejections(executable, request, request_document, root, provenance)
        else:
            with tempfile.TemporaryDirectory(prefix="actor-refresh-") as temporary:
                native_output = Path(temporary) / "native.json"
                subprocess.run([str(executable), manifest["command"], str(request),
                                "--out", str(native_output)], cwd=root, check=True)
                raw = native_output.read_bytes()
            result = json.loads(raw)
        assert result.get("oracle") == entry["oracle"], result.get("oracle")
        assert "error" not in result, result.get("error")
        assert result["provenance"]["git_revision"] == manifest["itgmania_revision"]
        assert result["provenance"]["git_dirty"] == "false"
        if "samples" in request_document:
            assert len(result["samples"]) == len(request_document["samples"]), request
        for missing_dependency in entry.get("absent_dependencies", []):
            missing = (root / missing_dependency).resolve()
            missing.relative_to(root)
            assert not missing.exists(), f"intentional missing resource now exists: {missing}"
        dependencies = []
        for relative_dependency in entry.get("dependencies", []):
            dependency = (root / relative_dependency).resolve()
            dependency.relative_to(root)
            dependencies.append({"path": relative_dependency, "sha256": digest(dependency)})
        if output.suffix == ".zst":
            raw = zstandard.ZstdCompressor(level=19).compress(raw)
        output.write_bytes(raw)
        receipt["fixtures"].append({**entry, "input_sha256": digest(request),
                                    "output_sha256": digest(output),
                                    "dependencies": dependencies,
                                    "native_provenance": result["provenance"]})
        print(f"{relative.name}: {time.monotonic() - started:.2f}s", flush=True)
    assert digest(executable) == executable_hash, "harness executable changed during capture"
    receipt_path = Path(manifest.get("receipt", "tests/fixtures/itgmania-actors/capture-manifest.json"))
    assert not receipt_path.is_absolute() and ".." not in receipt_path.parts
    destination = output_root / receipt_path
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n",
                           encoding="utf-8")


if __name__ == "__main__":
    main()
