"""Add an explicit-context CLI to an existing private codec oracle copy.

The original codec executable is preserved. The native engine implementations
stay unchanged. An explicit setup patch keeps one game identity for scoring and
noteskin discovery; the semantic evaluator modules remain unchanged.
"""
import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adapter-root", required=True, type=Path)
    parser.add_argument("--harness-source", required=True, type=Path)
    parser.add_argument("--itgmania-source", required=True, type=Path)
    parser.add_argument("--update", action="store_true",
                        help="Build the revised driver/setup patch after a completed first installation")
    args = parser.parse_args()
    root = args.adapter_root.resolve()
    harness = args.harness_source.resolve()
    engine = args.itgmania_source.resolve()
    adapter_path = root / "adapter.json"
    adapter = json.loads(adapter_path.read_bytes())
    assert adapter["build_exit_code"] == 0
    record_path = root / ("micro-adapter-v2.json" if args.update else "micro-adapter.json")
    assert not record_path.exists(), "resume or inspect the existing adapter build"
    if args.update:
        previous = json.loads((root / "micro-adapter.json").read_bytes())
        assert previous["build_exit_code"] == 0
        executable = Path(previous["binary"])
        assert sha(executable) == previous["binary_sha256"]
        shutil.copyfile(executable, root / "micro-control-v1-oracle.exe")
    else:
        executable = Path(adapter["binary"])
        assert sha(executable) == adapter["binary_sha256"]
        preserved = root / executable.name.replace("itgmania-harness-rs", "primary-codec-oracle")
        shutil.copyfile(executable, preserved)
        adapter["binary"] = str(preserved)
        adapter_path.write_bytes((json.dumps(adapter, indent=2) + "\n").encode())
    source = root / "source"
    for path in (harness / "src").glob("*.rs"):
        if path.name != "main.rs":
            assert (source / "src" / path.name).read_bytes() == path.read_bytes(), path.name
    driver = Path(__file__).with_name("oracle_micro_control.rs")
    shutil.copyfile(driver, source / "src/micro_control.rs")
    main_path = source / "src/main.rs"
    before = (harness / "src/main.rs").read_text(encoding="utf-8")
    anchor = '        Some("actor-conformance") => emit_actor_conformance(parse_actor_conformance_args(args)?),'
    assert before.count(anchor) == 1
    after = "mod micro_control;\n" + before.replace(anchor, anchor + '\n        Some("song-lua-micro-control") => micro_control::run(args).map_err(CliError::Usage),')
    assert main_path.read_text(encoding="utf-8") == (after if args.update else before)
    main_path.write_bytes(after.encode())
    patch = "".join(difflib.unified_diff(before.splitlines(True), after.splitlines(True),
                                        fromfile="harness/src/main.rs", tofile="adapter/src/main.rs"))
    (root / "micro-cli.patch").write_bytes(patch.encode())
    manifest = source / "Cargo.toml"
    text = manifest.read_text(encoding="utf-8")
    old_suffix = '+primary-codecs.micro-controls"' if args.update else '+primary-codecs"'
    assert text.count(old_suffix) == 1
    manifest.write_bytes(text.replace(old_suffix, '+primary-codecs.micro-controls2"').encode())
    bridge = source / "native/oracle_bridge.cpp"
    native_before = (harness / "native/oracle_bridge.cpp").read_text(encoding="utf-8")
    assert bridge.read_text(encoding="utf-8") == native_before
    old = ('    Game option_game{};\n    option_game.m_szName = "dance";\n'
           '    NoteSkinGlobalGuard option_skin_global(&option_skins, &option_game);')
    assert native_before.count(old) == 1
    native_after = native_before.replace(old,
        '    NoteSkinGlobalGuard option_skin_global(&option_skins, harness_score_game());')
    bridge.write_bytes(native_after.encode())
    native_patch = "".join(difflib.unified_diff(native_before.splitlines(True), native_after.splitlines(True),
        fromfile="harness/native/oracle_bridge.cpp", tofile="adapter/native/oracle_bridge.cpp"))
    (root / "micro-native-setup.patch").write_bytes(native_patch.encode())
    record = {"harness_revision": adapter["harness_revision"],
              "native_support_revision": adapter["native_baseline_revision"],
              "primary_codec_revision": adapter["itgmania_revision"],
              "driver_sha256": hashlib.sha256(driver.read_bytes().replace(b"\r\n", b"\n")).hexdigest(),
              "cli_patch_sha256": sha(root / "micro-cli.patch"),
              "native_setup_patch_sha256": sha(root / "micro-native-setup.patch"),
              "scope": "Explicit context driver and native game identity setup fix; unchanged native engine and song_lua_headless/semantic evaluator modules.",
              "state": "building"}
    record_path.write_bytes((json.dumps(record, indent=2) + "\n").encode())
    env = os.environ.copy()
    env["ITGMANIA_ROOT"] = str(harness / "vendor/itgmania")
    env["ITGMANIA_CODEC_ROOT"] = str(engine)
    started = time.monotonic()
    with record_path.with_suffix(".build.log").open("wb") as output:
        result = subprocess.run(["cargo", "+1.98.1", "build", "--manifest-path", str(manifest),
                                 "--target-dir", str(root / "target")], cwd=source, env=env,
                                stdout=output, stderr=subprocess.STDOUT)
    record.update(build_exit_code=result.returncode, seconds=time.monotonic() - started, state="finished")
    if result.returncode == 0:
        record.update(binary=str(executable), binary_sha256=sha(executable))
    record_path.write_bytes((json.dumps(record, indent=2) + "\n").encode())
    print(json.dumps(record), flush=True)
    raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
