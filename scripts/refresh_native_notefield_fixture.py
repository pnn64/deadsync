"""Refresh the focused full-song native geometry reference from bundled inputs.

This records native generation separately from its expensive compiled-song replay.
The latter remains an explicit long-song test, never an implied passing result.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile

import zstandard

from fixture_capture_hash import HASH_ENCODING, digest
from refresh_native_gameplay_controls import git, sha


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("harness", "harness-source", "native-source", "itgmania-bin", "output-root"):
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument("--existing-capture", type=Path)
    parser.add_argument("--existing-execution", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    recipe_path = root / "tests/fixtures/itgmania-song-lua-micro/notefield-refresh-inputs.json"
    recipe = json.loads(recipe_path.read_bytes())
    harness, native = args.harness.resolve(), args.itgmania_bin.resolve()
    assert subprocess.check_output([str(harness), "--version"], text=True).strip() == recipe["harness_version"]
    for source, revision in [(args.harness_source.resolve(), recipe["harness_revision"]),
                             (args.native_source.resolve(), recipe["native_revision"])]:
        assert git(source, "rev-parse", "HEAD") == revision
        assert not git(source, "status", "--porcelain", "--untracked-files=no")
    assert sha(native) == recipe["native_executable_sha256"]
    archive = root / recipe["archive"]
    assert sha(archive) == recipe["archive_sha256"]
    entry = recipe["fixtures"][0]
    output_root = args.output_root.resolve()
    with tempfile.TemporaryDirectory(prefix="native-notefield-inputs-") as temporary:
        inputs = Path(temporary).resolve()
        with zstandard.ZstdDecompressor().stream_reader(archive.open("rb")) as reader, tarfile.open(fileobj=reader, mode="r|") as tar:
            for member in tar:
                if member.name == "manifest.json" or member.name.startswith("song/"):
                    (inputs / member.name).resolve().relative_to(inputs)
                    tar.extract(member, inputs, filter="data")
        manifest = json.loads((inputs / "manifest.json").read_bytes())
        for member in manifest["files"]:
            if member["path"].startswith("song/"):
                assert sha(inputs / member["path"]) == member["sha256"]
        simfile = inputs / recipe["simfile"]
        assert sha(simfile) == recipe["simfile_sha256"]
        output = inputs / "native.json"
        if args.existing_capture:
            assert args.existing_execution
            execution = json.loads(args.existing_execution.read_bytes())
            assert execution["exit_code"] == 0
            assert execution["harness_binary_sha256"] == sha(harness)
            assert execution["native_executable_sha256"] == sha(native)
            assert execution["simfile_sha256"] == recipe["simfile_sha256"]
            command = execution["command"]
            for flag, value in [("--drawable-from-beat", entry["drawable_from_beat"]),
                                ("--until-beat", entry["until_beat"]), ("--capture-fps", 120)]:
                assert float(command[command.index(flag) + 1]) == value
            assert "--fixed-update-fps" not in command
            raw = args.existing_capture.read_bytes()
        else:
            command = [str(harness), "song-lua-trace", str(simfile), "--itgmania-bin", str(native),
                "--headless", "--notefield-samples", "--style", "single", "--difficulty", "Challenge",
                "--fallback-theme", recipe["fallback_theme"], "--player-options", recipe["player_options"],
                "--drawable-from-beat", str(entry["drawable_from_beat"]), "--until-beat", str(entry["until_beat"]),
                "--capture-fps", "120", "--timeout", "180", "--max-events", "1000000", "--out", str(output)]
            subprocess.run(command, check=True, cwd=args.harness_source.resolve())
            raw = output.read_bytes()
    capture = json.loads(raw)
    assert capture["harness_version"] == recipe["harness_version"]
    assert capture["oracle"] == "itgmania_notefield_render_trace"
    assert capture["input"]["native_source"]["revision"] == recipe["native_revision"]
    assert capture["input"]["native_source"]["modified_src"] is False
    assert capture["input"]["window_visible"] is False
    assert capture["input"]["constant_update_delta_seconds"] == 0
    assert capture["input"]["player_options"] == recipe["player_options"]
    assert capture["completion"]["reason"] == "beat-limit"
    for key in ["dropped_model_geometry_samples", "dropped_drawable_samples"]:
        assert capture[key] == 0
    assert capture["model_geometry_samples"] and capture["drawable_samples"]
    arrivals = set()
    caps = [set(), set()]
    for sample in capture["model_geometry_samples"]:
        state = sample["state"]
        context = state["note_context"]
        if (context["kind"] == "actor" and context["note_beat"] == 130.75
                and context["column"] == 3 and state["mesh_index"] == 0
                and state["render_target"] != 0 and context["draw_beat"] >= 130.2):
            arrivals.add(context["draw_beat"])
    for sample in capture["drawable_samples"]:
        state = sample["state"]
        context = state["note_context"]
        if context["part"] == 2 and state["render_target"] != 0:
            for index, (lower, upper) in enumerate([(121.91, 121.99), (129.91, 129.99)]):
                if lower <= context["draw_beat"] <= upper:
                    caps[index].add(context["draw_beat"])
    assert len(arrivals) >= 20 and all(caps), "capture lost the focused replay's native observation windows"
    assert capture["provenance"]["simfile_sha256"] == recipe["simfile_sha256"]
    assert capture["provenance"]["native_executable_sha256"] == sha(native)
    assert capture["provenance"]["capture_binary_sha256"] == sha(harness)
    destination = output_root / entry["output"]
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(zstandard.ZstdCompressor(level=19).compress(raw))
    record = {"schema_version": 1, "hash_encoding": HASH_ENCODING,
        "review_status": "captured-long-replay-deferred", "harness_version": recipe["harness_version"],
        "harness_revision": recipe["harness_revision"], "harness_binary_sha256": sha(harness),
        "native_revision": recipe["native_revision"], "native_executable_sha256": sha(native),
        "request_manifest_sha256": digest(recipe_path), "archive": recipe["archive"],
        "archive_sha256": recipe["archive_sha256"], "uncompressed_capture_sha256": hashlib.sha256(raw).hexdigest(),
        "fixtures": [{**entry, "output_sha256": digest(destination),
            "native_source": capture["input"]["native_source"],
            "models": len(capture["model_geometry_samples"]), "strips": len(capture["drawable_samples"])}]}
    receipt = output_root / "tests/fixtures/itgmania-song-lua-micro/notefield-capture-manifest.json"
    receipt.write_bytes((json.dumps(record, indent=2) + "\n").encode())
    print(f'Captured {record["fixtures"][0]["models"]} model draws and {record["fixtures"][0]["strips"]} strips; long compiled-song replay remains deferred.')


if __name__ == "__main__":
    main()
