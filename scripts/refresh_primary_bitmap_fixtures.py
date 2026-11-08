"""Refresh the defined primary bitmap observations from their bundled recipe.

The private codec adapter is explicitly identified. Inputs whose native palette
reads are undefined retain only their reviewed header observations.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def load(path):
    return json.loads(path.read_bytes())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adapter", required=True, type=Path)
    parser.add_argument("--itgmania-source", required=True, type=Path)
    parser.add_argument("--output-root", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    relative = Path("tests/fixtures/itgmania-primary-bitmaps/capture.json")
    recipe = load(root / relative)
    adapter = load(args.adapter.resolve())
    engine = args.itgmania_source.resolve()
    revision = subprocess.check_output(["git", "-C", str(engine), "rev-parse", "HEAD"],
                                       text=True).strip()
    assert revision == recipe["primary_codec_revision"] == adapter["itgmania_revision"]
    assert not subprocess.check_output(["git", "-C", str(engine), "status", "--porcelain",
                                        "--untracked-files=no"], text=True).strip()
    assert adapter["harness_revision"] == recipe["harness_revision"]
    assert adapter["native_baseline_revision"] == recipe["native_support_revision"]
    assert adapter["build_exit_code"] == 0
    executable = Path(adapter["binary"])
    assert sha(executable) == adapter["binary_sha256"]
    version = subprocess.check_output([str(executable), "--version"], text=True).strip()
    assert version == recipe["harness_version"]
    result = copy.deepcopy(recipe)
    for key, adapter_key in [("binary_sha256", "binary_sha256"),
                             ("adapter_build_sha256", "adapter_build_rs_sha256"),
                             ("adapter_patch_sha256", "adapter_patch_sha256")]:
        result[key] = adapter[adapter_key]
    for mode, key in [("texture_files", "cases"), ("texture_headers", "header_only")]:
        for case in result[key]:
            path = (root / case["source"]).resolve()
            path.relative_to(root)
            spec = {**case.get("request", {}), "file": str(path)}
            with tempfile.TemporaryDirectory(prefix="primary-bitmap-") as temporary:
                request = Path(temporary) / "request.json"
                output = Path(temporary) / "native.json"
                request.write_bytes(json.dumps({"name": "primary-bitmap-control", mode: [spec]}).encode())
                subprocess.run([str(executable), "actor-conformance", str(request), "--out", str(output)],
                               cwd=root, check=True, capture_output=True)
                native = load(output)
                assert native["provenance"]["git_revision"] == recipe["native_support_revision"]
                assert native["provenance"]["git_dirty"] == "false"
                assert len(native["cases"]) == 1
                case["native"] = native["cases"][0]
            case["source_sha256"] = sha(path)
    result["codec_source_sha256"] = [{"path": name, "sha256": sha(engine / name)} for name in [
        "src/RageSurface_Load_STB.cpp", "src/RageSurface_Load_STB.h",
        "src/RageSurface_Load_PNG.cpp", "src/RageSurface_Load_GIF.cpp",
        "src/RageSurface_Load_BMP.cpp", "src/RageSurface_Load_JPEG.cpp",
        "extern/stb_image.c", "extern/stb/stb_image.h"]]
    assert sha(executable) == adapter["binary_sha256"]
    destination = args.output_root.resolve() / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes((json.dumps(result, indent=2) + "\n").encode())
    print(f'Refreshed {len(result["cases"])} primary bitmap cases and {len(result["header_only"])} headers.')


if __name__ == "__main__":
    main()
