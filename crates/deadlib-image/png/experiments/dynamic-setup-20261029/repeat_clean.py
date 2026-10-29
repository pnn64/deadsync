"""Build both crate versions into one caller and retain alternating cycle probes."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib

EVIDENCE = Path(__file__).resolve().parent
ROOT = EVIDENCE.parents[4]
PNG = ROOT / "crates/deadlib-image/png"
LABEL = os.environ.get("DEADLIB_PNG_COMPARE_LABEL", "repeat-clean-focus")
assert LABEL.replace("-", "").isalnum()
WORK = ROOT / "target/png-dynamic-setup-20261029" / LABEL
BASE = "a5ca368eb"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git_file(path):
    return subprocess.check_output(["git", "show", BASE + ":" + path], cwd=ROOT)


def main():
    manifest = EVIDENCE / (LABEL + ".json")
    assert not manifest.exists()
    WORK.mkdir(parents=True, exist_ok=False)
    paths = subprocess.check_output(["git", "ls-tree", "-r", "--name-only", BASE, "--",
                                     "crates/deadlib-image/png/src"], cwd=ROOT, text=True).splitlines()
    before = {}
    for path in paths:
        destination = WORK / "baseline/src" / path.split("/src/", 1)[1]
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(git_file(path))
        before[path] = sha(destination)
    selected_hashes = {}
    snapshot = json.loads((EVIDENCE / "repeat-clean.json").read_text())["source_sha256"]
    added = b"".join(line[1:] + b"\n" for line in (EVIDENCE / "repeat-clean.patch").read_bytes().splitlines()
                     if line.startswith(b"+") and not line.startswith(b"+++"))
    assert added.startswith(b"        if distance == 1 {")
    for path, digest in before.items():
        data = git_file(path)
        if path.endswith("/zlib/inflate.rs"):
            marker = b"    fn copy_match(&mut self, distance: usize, output: &mut [u8]) {\n"
            assert data.count(marker) == 1
            data = data.replace(marker, marker + added)
        assert hashlib.sha256(data).hexdigest() == snapshot[path], path
        destination = WORK / "selected/src" / path.split("/src/", 1)[1]
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        selected_hashes[path] = sha(destination)
    image_path = json.dumps((ROOT / "crates/deadlib-image").as_posix())
    (WORK / "baseline/Cargo.toml").write_text(
        '[package]\nname = "png-lane-baseline"\nversion = "0.1.0"\nedition = "2024"\n'
        '[dependencies]\ndeadlib-image = { path = ' + image_path + ' }\n')
    (WORK / "selected/Cargo.toml").write_text(
        '[package]\nname = "png-lane-selected"\nversion = "0.1.0"\nedition = "2024"\n'
        '[dependencies]\ndeadlib-image = { path = ' + image_path + ' }\n')
    source = WORK / "src"
    source.mkdir()
    (source / "main.rs").write_bytes((EVIDENCE / "repeat.rs").read_bytes().replace(
        b'../../examples/support/cpu.rs', b'cpu.rs').replace(b'../../tests/support/chunks.rs', b'chunks.rs'))
    shutil.copy2(PNG / "examples/support/cpu.rs", source / "cpu.rs")
    shutil.copy2(PNG / "tests/support/chunks.rs", source / "chunks.rs")
    settings = tomllib.loads((ROOT / "Cargo.toml").read_text())
    profile = dict(settings["profile"]["release"])
    profile.update(settings["profile"].get("bench", {}))
    profile_lines = "\n".join(key + " = " + json.dumps(value) for key, value in profile.items()
                              if isinstance(value, (str, int, bool)) and key != "inherits")
    (WORK / "Cargo.toml").write_text(
        '[workspace]\nmembers = ["baseline", "selected"]\nresolver = "3"\n'
        '[package]\nname = "png-lane-compare"\nversion = "0.1.0"\nedition = "2024"\n'
        '[dependencies]\nbaseline = { package = "png-lane-baseline", path = "baseline" }\n'
        'selected = { package = "png-lane-selected", path = "selected" }\n'
        'flate2 = \"1.1.10\"\n'
        '[profile.bench]\n' + profile_lines + '\n')
    current = selected_hashes
    command = ["cargo", "build", "--offline", "--manifest-path", str(WORK / "Cargo.toml"),
               "--profile", "bench", "--target-dir", str(WORK / "build"), "--message-format", "json"]
    result = subprocess.run(command, cwd=ROOT, env=dict(os.environ, CARGO_BUILD_JOBS="4"), capture_output=True)
    log = EVIDENCE / (LABEL + "-build.log")
    log.write_bytes(result.stdout + result.stderr)
    assert result.returncode == 0, result.stderr.decode(errors="replace")
    artifacts = [json.loads(line) for line in result.stdout.splitlines()]
    binary = Path(next(a["executable"] for a in reversed(artifacts)
                       if a.get("reason") == "compiler-artifact" and a.get("executable")))
    saved = WORK / binary.name
    shutil.copy2(binary, saved)
    assert current == {name: sha(WORK / "selected/src" / name.split("/src/", 1)[1]) for name in current}
    record = dict(source_base=BASE, driver_sha256=sha(Path(__file__)), baseline_sha256=before, selected_sha256=current,
                  shared_image_sha256=sha(ROOT / "crates/deadlib-image/src/lib.rs"),
                  harness_sha256=sha(EVIDENCE / "repeat.rs"), cpu_sha256=sha(source / "cpu.rs"),
                  chunks_sha256=sha(source / "chunks.rs"),
                  generated_source_sha256=sha(source / "main.rs"), profile=profile, command=command,
                  executable=str(saved), executable_sha256=sha(saved), build_log_sha256=sha(log), outputs=[])
    for run in range(1, 4):
        result = subprocess.run([str(saved)], cwd=ROOT, capture_output=True)
        assert result.returncode == 0, result.stderr.decode(errors="replace")
        log = EVIDENCE / f"{LABEL}-run{run}.log"
        log.write_bytes(result.stdout + result.stderr)
        record["outputs"].append(dict(path=log.name, sha256=sha(log)))
        manifest.write_text(json.dumps(record, indent=2) + "\n")
        print(f"in-process comparison {run} passed", flush=True)


if __name__ == "__main__":
    main()
