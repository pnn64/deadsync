"""Refresh short Lua traces with explicit committed context/input recipes.

The input adapter calls the current, unchanged harness evaluator. Real simfiles
use its normal CLI. Capture failures remain reported and are never relabeled as
passing. Outputs are staged under the requested directory for comparison.
"""
import argparse
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

import zstandard

from fixture_capture_hash import HASH_ENCODING, digest


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for item in value:
            yield from strings(item)
    elif isinstance(value, dict):
        for item in value.values():
            yield from strings(item)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adapter", required=True, type=Path)
    parser.add_argument("--manifest", type=Path, default=Path(
        "tests/fixtures/itgmania-song-lua-micro/context-refresh-inputs.json"))
    parser.add_argument("--output-root", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    manifest_path = (root / args.manifest).resolve()
    manifest_path.relative_to(root)
    manifest = json.loads(manifest_path.read_bytes())
    adapter = json.loads(args.adapter.resolve().read_bytes())
    assert adapter["build_exit_code"] == 0
    assert digest(root / "scripts/oracle_micro_control.rs") == adapter["driver_sha256"]
    executable = Path(adapter["binary"])
    assert digest(executable) == adapter["binary_sha256"]
    version = subprocess.check_output([str(executable), "--version"], text=True).strip()
    assert version == manifest["harness_version"]
    for key in ("harness_revision", "native_support_revision", "primary_codec_revision"):
        assert adapter[key] == manifest[key], key
    env = os.environ.copy()
    env["ITGMANIA_SONG_LUA_NOTESKIN_ROOT"] = str(root / manifest["noteskin_root"])
    for key in ("ITGMANIA_SONG_LUA_NOTESKIN", "ITGMANIA_SONG_LUA_JUDGMENT", "ITGMANIA_SONG_LUA_RAW_TRACE"):
        env.pop(key, None)
    output_root = args.output_root.resolve()
    receipt = {"schema_version": 1, "hash_encoding": HASH_ENCODING,
               "review_status": "staged-unreviewed",
               "harness_version": version, "harness_revision": adapter["harness_revision"],
               "harness_binary_sha256": adapter["binary_sha256"],
               "native_support_revision": adapter["native_support_revision"],
               "primary_codec_revision": adapter["primary_codec_revision"],
               "driver_sha256": adapter["driver_sha256"], "cli_patch_sha256": adapter["cli_patch_sha256"],
               "native_setup_patch_sha256": adapter["native_setup_patch_sha256"],
               "request_manifest_sha256": digest(manifest_path), "fixtures": [], "failures": []}
    with tempfile.TemporaryDirectory(prefix="micro-native-probe-") as temporary:
        request = Path(temporary) / "input.json"
        output = Path(temporary) / "native.json"
        request.write_bytes(json.dumps({"schema_version": 1, "name": "source-identity", "samples": [0],
                                        "root": {"kind": "frame", "name": "Root", "children": []}}).encode())
        probe = subprocess.run([str(executable), "actor-conformance", str(request), "--out", str(output)],
                               cwd=root, capture_output=True, encoding="utf-8")
        assert probe.returncode == 0, probe.stderr
        provenance = json.loads(output.read_bytes())["provenance"]
        assert provenance["git_revision"] == adapter["native_support_revision"]
        assert provenance["git_dirty"] == "false"
    for entry in manifest["fixtures"]:
        start = time.monotonic()
        request = (root / entry["input"]).resolve()
        request.relative_to(root)
        song_root = (root / entry["song_root"]).resolve()
        song_root.relative_to(root)
        relative = Path(entry["output"])
        assert not relative.is_absolute() and ".." not in relative.parts
        output = output_root / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        try:
            with tempfile.TemporaryDirectory(prefix="micro-context-") as temporary:
                native_output = Path(temporary) / "native.json"
                if entry["kind"] == "context":
                    command = [str(executable), "song-lua-micro-control", str(request),
                               "--out", str(native_output)]
                else:
                    command = [str(executable), "song-lua-semantic-baseline", str(song_root),
                               "--simfile", request.relative_to(song_root).as_posix(), "--out", temporary,
                               "--beat-step", str(entry["beat_step"]), "--until-beat", str(entry["until_beat"]),
                               "--difficulty", entry["difficulty"], "--steps-type", entry["steps_type"],
                               "--random-seed", str(entry["random_seed"]), "--max-events", str(entry["max_events"])]
                result = subprocess.run(command, cwd=root, env=env, capture_output=True, encoding="utf-8")
                assert result.returncode == 0, result.stderr
                if entry["kind"] == "simfile":
                    generated = json.loads((Path(temporary) / "_semantic_manifest.json").read_bytes())["simfiles"]
                    assert len(generated) == 1, generated
                    if generated[0]["status"] != "ok":
                        failed_path = Path(temporary) / generated[0].get("fixture", "missing")
                        detail = json.loads(failed_path.read_bytes()).get("runtime_errors") if failed_path.is_file() else generated
                        raise AssertionError(detail)
                    native_output = Path(temporary) / generated[0]["fixture"]
                raw = native_output.read_bytes()
                document = json.loads(raw)
                assert document["harness_version"] == version
                assert document["runtime_errors"] == [], document["runtime_errors"]
                assert document["dropped_events"] == 0
                assert document["capabilities"]["runtime_complete"]
                assert abs(document["end_position"]["beat"] - entry["until_beat"]) < 0.001
                # A symbolic context cannot replace a capture that obtained its
                # timing from a real simfile. Recreating the Lua roots alone
                # would silently discard native timing/step coverage.
                previous = root / relative
                if previous.is_file():
                    previous_raw = previous.read_bytes()
                    if previous.suffix == ".zst":
                        with zstandard.ZstdDecompressor().stream_reader(io.BytesIO(previous_raw)) as stream:
                            previous_raw = stream.read()
                    previous_document = json.loads(previous_raw)
                    if previous_document.get("arrow_timing") == "native":
                        assert document.get("arrow_timing") == "native", (
                            "native simfile timing was lost; supply the committed simfile instead of a symbolic context")
                dependencies = {request}
                for value in strings(document):
                    if value.startswith("song:/"):
                        path = (song_root / value[6:]).resolve()
                        path.relative_to(root)
                        if path.is_file():
                            dependencies.add(path)
                if entry["kind"] == "context":
                    simfile = json.loads(request.read_bytes()).get("simfile")
                    if simfile:
                        path = (root / simfile).resolve()
                        path.relative_to(root)
                        assert path.is_file(), simfile
                        dependencies.add(path)
                for loaded in document["loaded_lua_files"]:
                    if loaded.startswith("song:/"):
                        path = (song_root / loaded[6:]).resolve()
                        path.relative_to(root)
                        assert path.is_file(), loaded
                        dependencies.add(path)
                    else:
                        assert loaded.startswith(("noteskin:/", "theme:/")), loaded
                for file in document.get("noteskin_reference", {}).get("files", []):
                    path = (root / manifest["noteskin_root"] / file["path"]).resolve()
                    path.relative_to(root)
                    dependencies.add(path)
                if output.suffix == ".zst":
                    raw = zstandard.ZstdCompressor(level=19).compress(raw)
                output.write_bytes(raw)
                receipt["fixtures"].append({**entry, "input_sha256": digest(request),
                    "output_sha256": digest(output), "native_provenance": provenance,
                    "dependencies": [{"path": path.relative_to(root).as_posix(), "sha256": digest(path)}
                                     for path in sorted(dependencies)]})
            print(f'{relative}: {time.monotonic() - start:.2f}s', flush=True)
        except (AssertionError, OSError, ValueError) as error:
            failure = {"output": entry["output"], "error": str(error)}
            receipt["failures"].append(failure)
            print("FAILED " + json.dumps(failure), flush=True)
    assert digest(executable) == adapter["binary_sha256"]
    destination = output_root / manifest_path.parent.relative_to(root) / "context-capture-manifest.json"
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes((json.dumps(receipt, indent=2) + "\n").encode())
    print(f'{len(receipt["fixtures"])} complete, {len(receipt["failures"])} failed', flush=True)
    raise SystemExit(bool(receipt["failures"]))


if __name__ == "__main__":
    main()
