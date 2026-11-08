"""Stage direct native gameplay observations using the pinned harness installer.

Inputs come from a committed archive. The official harness prepares a private
offscreen install with a zero-beat capture; the two committed observers then run
there without wrapping authored Lua. Outputs remain staged for numerical review.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile
import time

import zstandard

from fixture_capture_hash import HASH_ENCODING, digest


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git(path, *args):
    return subprocess.check_output(["git", "-C", str(path), *args], text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("harness", "harness-source", "native-source", "itgmania-bin", "output-root"):
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument("--prepared-install", type=Path,
                        help="Resume a terminal generation attempt in its verified private install")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    manifest_path = root / "tests/fixtures/itgmania-song-lua-micro/gameplay-refresh-inputs.json"
    manifest = json.loads(manifest_path.read_bytes())
    harness, native = args.harness.resolve(), args.itgmania_bin.resolve()
    for source, revision in [(args.harness_source.resolve(), manifest["harness_revision"]),
                             (args.native_source.resolve(), manifest["native_revision"])]:
        assert git(source, "rev-parse", "HEAD") == revision
        assert not git(source, "status", "--porcelain", "--untracked-files=no"), source
    assert subprocess.check_output([str(harness), "--version"], text=True).strip() == manifest["harness_version"]
    assert sha(native) == manifest["native_executable_sha256"]
    harness_hash = sha(harness)
    output = args.output_root.resolve()
    output.mkdir(parents=True, exist_ok=bool(args.prepared_install))
    archive = root / manifest["archive"]
    assert sha(archive) == manifest["archive_sha256"]
    inputs = output / "inputs"
    if not args.prepared_install:
        inputs.mkdir()
        with zstandard.ZstdDecompressor().stream_reader(archive.open("rb")) as reader, tarfile.open(fileobj=reader, mode="r|") as tar:
            for member in tar:
                if member.name == "manifest.json" or member.name.startswith("song/"):
                    (inputs / member.name).resolve().relative_to(inputs)
                    tar.extract(member, inputs, filter="data")
    archived = json.loads((inputs / "manifest.json").read_bytes())
    for member in archived["files"]:
        if member["path"].startswith("song/"):
            assert sha(inputs / member["path"]) == member["sha256"], member["path"]
    simfile = inputs / manifest["simfile"]
    assert sha(simfile) == manifest["simfile_sha256"]
    # A successful, tiny official capture prepares all preferences, the exact
    # theme overlays and a private Program directory. Its PID identifies only
    # this install, even when another harness is running concurrently.
    command = [str(harness), "song-lua-trace", str(simfile), "--itgmania-bin", str(native),
               "--headless", "--notefield-samples", "--style", "single", "--difficulty", "Challenge",
               "--fallback-theme", manifest["fallback_theme"], "--player-options", manifest["player_options"],
               "--drawable-from-beat", "0", "--until-beat", "0", "--capture-fps", "120",
               "--fixed-update-fps", "120", "--timeout", "45", "--max-events", "1000000",
               "--keep-run-dir", "--out", str(output / "preparation.json")]
    if args.prepared_install:
        install = args.prepared_install.resolve()
        install.relative_to((Path(tempfile.gettempdir()) / "itgmania-harness-rs").resolve())
    else:
        with (output / "preparation.log").open("wb") as log:
            prepared = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT,
                                        cwd=args.harness_source.resolve())
            prep_pid = prepared.pid
            code = prepared.wait()
        assert code == 0, (output / "preparation.log").read_text(errors="replace")
        installs = list((Path(tempfile.gettempdir()) / "itgmania-harness-rs").glob(f"song-lua-{prep_pid}-*"))
        assert len(installs) == 1, installs
        install = installs[0].resolve()
    preparation = json.loads((output / "preparation.json").read_bytes())
    assert preparation["input"]["native_source"]["revision"] == manifest["native_revision"]
    (output / "preparation-state.json").write_bytes((json.dumps({"install": str(install),
        "harness_binary_sha256": harness_hash, "native_executable_sha256": sha(native)}, indent=2) + "\n").encode())
    staged_binary = install / "Program" / native.name
    assert sha(staged_binary) == sha(native)
    config_path = install / "Themes/itgmania-harness/Scripts/00 HarnessConfig.lua"
    original_config = config_path.read_text(encoding="utf-8")
    record = {"schema_version": 1, "hash_encoding": HASH_ENCODING, "review_status": "staged-unreviewed",
              "harness_version": manifest["harness_version"], "harness_revision": manifest["harness_revision"],
              "harness_binary_sha256": harness_hash, "native_revision": manifest["native_revision"],
              "native_executable_sha256": sha(native), "request_manifest_sha256": digest(manifest_path),
              "archive": manifest["archive"], "archive_sha256": manifest["archive_sha256"], "fixtures": []}
    for entry in manifest["fixtures"]:
        observer = root / entry["observer"]
        for member in archived["files"]:
            if member["path"].startswith("song/"):
                staged = install / "Songs/Harness/Probe" / member["path"][5:]
                assert sha(staged) == member["sha256"], member["path"]
        probe = install / "Themes/itgmania-harness/Scripts/03 SongLuaTrace.lua"
        result_path = install / entry["raw_result"]
        reuse = bool(args.prepared_install and result_path.is_file())
        if reuse:
            assert sha(probe) == sha(observer), "cannot reuse a capture from a different observer"
        probe.write_bytes(observer.read_bytes())
        assert sha(probe) == sha(observer)
        config, count = re.subn(r"until_beat = [0-9.]+", f'until_beat = {entry["until_beat"]}', original_config)
        assert count == 1
        config_path.write_bytes(config.encode())
        assert reuse or not result_path.exists(), result_path
        env = os.environ.copy()
        env.update(ITGMANIA_HARNESS_HEADLESS="1", ITGMANIA_HARNESS_HEADLESS_GRAPHICS="1")
        started = time.monotonic()
        with (output / (entry["name"] + ".engine.log")).open("ab" if reuse else "wb") as log:
            if reuse:
                child = None
            else:
                child = subprocess.Popen([str(staged_binary), "--game=dance", "--theme=itgmania-harness"],
                                         cwd=staged_binary.parent, env=env, stdout=log, stderr=subprocess.STDOUT)
            try:
                while not result_path.is_file():
                    assert child.poll() is None, f"native engine exited: {child.returncode}; {install}"
                    assert not (install / "crashinfo.txt").exists(), f"native crash report: {install}"
                    assert time.monotonic() - started < 180, f"native observer timeout: {install}"
                    time.sleep(0.025)
            finally:
                if child and child.poll() is None:
                    child.kill()
                if child:
                    child.wait()
        raw = result_path.read_bytes()
        document = json.loads(raw)
        assert not document.get("observer_error"), document.get("observer_error")
        assert document["native_source"]["revision"] == manifest["native_revision"]
        assert document["native_source"]["modified_src"] is False
        assert document["input"]["headless_graphics"] is True
        assert document["input"]["window_visible"] is False
        # ITGmania persists preferences with six decimals during startup. The
        # original fixed-120 captures used this same serialized native input.
        assert abs(document["input"]["constant_update_delta_seconds"] - manifest["inputs"]["fixed_update_delta_seconds"]) < 1e-9
        assert abs(document["input"]["global_offset_seconds"] - manifest["inputs"]["global_offset"]) < 1e-7
        assert len(document["frames"]) >= entry["minimum_frames"]
        raw_path = output / (entry["name"] + ".raw.json.zst")
        raw_path.write_bytes(zstandard.ZstdCompressor(level=19).compress(raw))
        normalized = {"oracle": "native ITGmania ScreenGameplay, actual song Lua; after-draw observer",
                      "itgmania_version": document["version"], "harness_version": manifest["harness_version"],
                      "native_source": document["native_source"], "inputs": manifest["inputs"],
                      "observed_inputs": document["input"], "simfile_sha256": sha(simfile),
                      "lua_sha256": sha(install / "Songs/Harness/Probe" / manifest["lua"][5:]), "native_exe_sha256": sha(native),
                      "raw_capture_sha256": hashlib.sha256(raw).hexdigest(),
                      "observer_sha256": sha(observer), "observer_lf_sha256": digest(observer),
                      "frames": document["frames"]}
        if "cap_windows" in entry:
            caps = []
            for lower, upper, column in entry["cap_windows"]:
                groups = {}
                for strip in document["strips"]:
                    state = strip["state"]
                    context = state.get("note_context", {})
                    if (lower <= strip["beat"] <= upper and context.get("column") == column
                            and state.get("render_target") == 0 and context.get("part") in [0, 2]
                            and len(strip["vertices"]) == entry["cap_vertices"][context["part"] // 2]):
                        groups.setdefault(strip["beat"], {})[context["part"]] = strip["vertices"]
                pairs = [(beat, pair) for beat, pair in sorted(groups.items()) if 0 in pair and 2 in pair]
                assert pairs, f"missing complete native caps at {lower}..{upper}, column {column}"
                beat, pair = pairs[0]
                caps.append({"beat": beat, "top": pair[0], "bottom": pair[2]})
            normalized["caps"] = caps
        if "window_beats" in entry:
            normalized.update({key: entry[key] for key in ["window_beats", "first_note", "neighbor_note"]})
        destination = output / entry["output"]
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes((json.dumps(normalized, indent=2) + "\n").encode())
        record["fixtures"].append({**entry, "output_sha256": digest(destination),
            "observer_sha256": digest(observer), "raw_capture_sha256": hashlib.sha256(raw).hexdigest(),
            "native_source": document["native_source"]})
        print(f'{entry["name"]}: {len(document["frames"])} frames, {time.monotonic() - started:.2f}s', flush=True)
    assert sha(harness) == harness_hash and sha(native) == manifest["native_executable_sha256"]
    (output / "gameplay-capture-manifest.json").write_bytes((json.dumps(record, indent=2) + "\n").encode())


if __name__ == "__main__":
    main()
