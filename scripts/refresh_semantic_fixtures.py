"""Regenerate short semantic controls from committed simfiles.

The request manifest pins the sampling recipe. The capture receipt pins the
harness, its native source baseline, all observed repository resources, and the
resulting bytes. Ordinary tests consume those files without a harness checkout.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

import zstandard

from fixture_capture_hash import HASH_ENCODING, digest


def read_json(path):
    raw = path.read_bytes()
    if path.suffix == ".zst":
        with zstandard.ZstdDecompressor().stream_reader(io.BytesIO(raw)) as stream:
            raw = stream.read()
    return json.loads(raw)


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for item in value:
            yield from strings(item)
    elif isinstance(value, dict):
        for item in value.values():
            yield from strings(item)


def repository_path(root, relative):
    path = (root / relative).resolve()
    path.relative_to(root)
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--harness", required=True, type=Path)
    parser.add_argument("--harness-source", required=True, type=Path)
    parser.add_argument("--manifest", type=Path, default=Path(
        "tests/fixtures/itgmania-song-lua-micro/refresh-inputs.json"))
    parser.add_argument("--output-root", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    source = args.harness_source.resolve()
    executable = args.harness.resolve()
    manifest_path = repository_path(root, args.manifest)
    manifest = read_json(manifest_path)
    version = subprocess.check_output([str(executable), "--version"],
                                      text=True, encoding="utf-8").strip()
    revision = subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"],
                                       text=True, encoding="utf-8").strip()
    assert version == manifest["harness_version"], (version, manifest["harness_version"])
    assert revision == manifest["harness_revision"], (revision, manifest["harness_revision"])
    assert not subprocess.check_output(
        ["git", "-C", str(source), "status", "--porcelain", "--untracked-files=no"],
        text=True, encoding="utf-8").strip(), "harness tracked sources are modified"
    executable_hash = digest(executable)
    receipt = {"schema_version": 1, "hash_encoding": HASH_ENCODING, "harness_version": version,
               "harness_revision": revision, "harness_binary_sha256": executable_hash,
               "request_manifest_sha256": digest(manifest_path), "fixtures": []}
    output_root = args.output_root.resolve()
    env = os.environ.copy()
    # The native defaults use vendor PNGs. Pin the exact resource bytes consumed
    # by DeadSync, even where the images have identical dimensions and pixels.
    env["ITGMANIA_SONG_LUA_NOTESKIN_ROOT"] = str(repository_path(root, manifest["noteskin_root"]))
    for key in ("ITGMANIA_SONG_LUA_NOTESKIN", "ITGMANIA_SONG_LUA_JUDGMENT",
                "ITGMANIA_SONG_LUA_RAW_TRACE"):
        env.pop(key, None)
    for entry in manifest["fixtures"]:
        request = repository_path(root, entry["input"])
        song_root = repository_path(root, entry["song_root"])
        simfile = request.relative_to(song_root)
        relative = Path(entry["output"])
        assert not relative.is_absolute() and ".." not in relative.parts
        output = output_root / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        started = time.monotonic()
        case_env = env.copy()
        if "noteskin" in entry:
            case_env["ITGMANIA_SONG_LUA_NOTESKIN"] = entry["noteskin"]
        if "judgment" in entry:
            case_env["ITGMANIA_SONG_LUA_JUDGMENT"] = str(repository_path(root, entry["judgment"]))
        with tempfile.TemporaryDirectory(prefix="semantic-refresh-") as temporary:
            command = [str(executable), "song-lua-semantic-baseline", str(song_root),
                       "--simfile", simfile.as_posix(), "--out", temporary,
                       "--beat-step", str(entry["beat_step"]),
                       "--until-beat", str(entry["until_beat"]),
                       "--difficulty", entry["difficulty"],
                       "--steps-type", entry["steps_type"],
                       "--random-seed", str(entry["random_seed"]),
                       "--max-events", str(entry["max_events"])]
            result = subprocess.run(command, cwd=root, env=case_env,
                                    capture_output=True, encoding="utf-8")
            assert result.returncode == 0, (command, result.stdout, result.stderr)
            generation = read_json(Path(temporary) / "_semantic_manifest.json")
            assert generation["itgmania"]["git_revision"] == manifest["itgmania_revision"]
            assert generation["itgmania"]["git_dirty"] == "false"
            generated = generation["simfiles"]
            assert len(generated) == 1 and generated[0]["status"] == "ok", generated
            raw = (Path(temporary) / generated[0]["fixture"]).read_bytes()
            capture = json.loads(raw)
            assert capture["harness_version"] == version
            assert capture["oracle"] == "itgmania_song_lua_headless_semantic_trace"
            assert capture["runtime_errors"] == [], capture["runtime_errors"]
            assert capture["dropped_events"] == 0
            assert capture["capabilities"]["runtime_complete"]
            assert abs(capture["end_position"]["beat"] - entry["until_beat"]) < 0.001
            dependencies = {request}
            for value in strings(capture):
                if value.startswith("song:/"):
                    path = repository_path(root, song_root / value[6:])
                    if path.is_file():
                        dependencies.add(path)
            for file in capture.get("noteskin_reference", {}).get("files", []):
                path = repository_path(root, Path(manifest["noteskin_root"]) / file["path"])
                # This field is the native oracle's exact-byte hash. The receipt
                # below uses portable text hashes for checkout compatibility.
                assert hashlib.sha256(path.read_bytes()).hexdigest() == file["sha256"], path
                dependencies.add(path)
            if "judgment" in entry:
                dependencies.add(repository_path(root, entry["judgment"]))
            for loaded in capture["loaded_lua_files"]:
                assert loaded.startswith("song:/"), loaded
                assert repository_path(root, song_root / loaded[6:]).is_file(), loaded
            if output.suffix == ".zst":
                raw = zstandard.ZstdCompressor(level=19).compress(raw)
            output.write_bytes(raw)
            receipt["fixtures"].append({**entry, "output_sha256": digest(output),
                "native_provenance": generation["itgmania"],
                "dependencies": [{"path": path.relative_to(root).as_posix(),
                                  "sha256": digest(path)} for path in sorted(dependencies)]})
        print(f"{relative.name}: {time.monotonic() - started:.2f}s", flush=True)
    assert digest(executable) == executable_hash, "harness executable changed during capture"
    destination = output_root / manifest_path.parent.relative_to(root) / "capture-manifest.json"
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n",
                           encoding="utf-8")


if __name__ == "__main__":
    main()
