"""Export committed RGBA goldens from the current recorded native captures."""
import argparse
import json
from pathlib import Path

from audit_fixture_freshness import load
from fixture_capture_hash import digest


def relative_path(value):
    path = Path(value)
    assert not path.is_absolute() and ".." not in path.parts, value
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=Path(
        "tests/fixtures/itgmania-song-lua-micro/native-pixel-exports.json"))
    parser.add_argument("--output-root", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    contract = load(root / "tests/fixtures/oracle-contract.json")
    manifest = load(root / relative_path(str(args.manifest)))
    for key in ("harness_version", "harness_revision"):
        assert manifest[key] == contract[key], key
    captures = {}
    for group in contract["groups"]:
        receipt = load(root / group["captures"])
        for key in ("harness_version", "harness_revision"):
            assert receipt[key] == contract[key], key
        for entry in receipt["fixtures"]:
            captures[entry["output"]] = entry["output_sha256"]
    documents = {}
    for entry in manifest["exports"]:
        capture = entry["capture"]
        if capture not in documents:
            path = root / relative_path(capture)
            assert digest(path) == captures[capture], f"capture changed: {capture}"
            documents[capture] = load(path)
        value = documents[capture]
        assert value["cases"][entry["case_index"]]["name"] == entry["case_name"]
        assert entry["pointer"].startswith(f'/cases/{entry["case_index"]}/')
        for key in entry["pointer"].split("/")[1:]:
            value = value[int(key)] if isinstance(value, list) else value[key]
        assert all(len(pixel) == 4 and all(type(byte) is int and 0 <= byte <= 255
                                         for byte in pixel) for pixel in value)
        raw = bytes(byte for pixel in value for byte in pixel)
        output = args.output_root.resolve() / relative_path(entry["output"])
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(raw)
    print(f'Exported {len(manifest["exports"])} native RGBA files.')


if __name__ == "__main__":
    main()
