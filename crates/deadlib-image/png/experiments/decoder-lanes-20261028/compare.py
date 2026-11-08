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
LABEL = os.environ.get("DEADLIB_PNG_COMPARE_LABEL", "in-process")
assert LABEL.replace("-", "").isalnum()
WORK = ROOT / "target/png-decoder-lanes-20261028" / LABEL
BASE = "01e641a59"


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
    image_path = json.dumps((ROOT / "crates/deadlib-image").as_posix())
    (WORK / "baseline/Cargo.toml").write_text(
        '[package]\nname = "png-lane-baseline"\nversion = "0.1.0"\nedition = "2024"\n'
        '[dependencies]\ndeadlib-image = { path = ' + image_path + ' }\n')
    source = WORK / "src"
    source.mkdir()
    (source / "main.rs").write_bytes((EVIDENCE / "compare.rs").read_bytes().replace(
        b'../../examples/support/cpu.rs', b'cpu.rs'))
    shutil.copy2(PNG / "examples/support/cpu.rs", source / "cpu.rs")
    settings = tomllib.loads((ROOT / "Cargo.toml").read_text())
    profile = dict(settings["profile"]["release"])
    profile.update(settings["profile"].get("bench", {}))
    profile_lines = "\n".join(key + " = " + json.dumps(value) for key, value in profile.items()
                              if isinstance(value, (str, int, bool)) and key != "inherits")
    (WORK / "Cargo.toml").write_text(
        '[workspace]\nmembers = ["baseline"]\nresolver = "3"\n'
        '[package]\nname = "png-lane-compare"\nversion = "0.1.0"\nedition = "2024"\n'
        '[dependencies]\nbaseline = { package = "png-lane-baseline", path = "baseline" }\n'
        'selected = { package = "deadlib-image-png", path = ' + json.dumps(PNG.as_posix()) + ' }\n'
        '[profile.bench]\n' + profile_lines + '\n')
    current = {p.relative_to(ROOT).as_posix(): sha(p) for p in sorted((PNG / "src").rglob("*.rs"))}
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
    assert current == {p.relative_to(ROOT).as_posix(): sha(p) for p in sorted((PNG / "src").rglob("*.rs"))}
    record = dict(source_base=BASE, driver_sha256=sha(Path(__file__)), baseline_sha256=before, selected_sha256=current,
                  shared_image_sha256=sha(ROOT / "crates/deadlib-image/src/lib.rs"),
                  harness_sha256=sha(EVIDENCE / "compare.rs"), cpu_sha256=sha(source / "cpu.rs"),
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
