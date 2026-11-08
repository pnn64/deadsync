"""Measure replicated short tables using verified immutable source snapshots."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tomllib

EVIDENCE = Path(__file__).resolve().parent
ROOT = EVIDENCE.parents[4]
PNG = ROOT / "crates/deadlib-image/png"
BASE = "a5ca368eb"
WORK = ROOT / "target/png-dynamic-setup-20261029/short-tree-probe"
WRAPPER = b'\npub fn __bench_setup(lengths: &[u8]) { std::hint::black_box(Huffman::new(lengths, false, false).unwrap()); }\n'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    manifest = EVIDENCE / "replication.json"
    assert not manifest.exists()
    WORK.mkdir(parents=True, exist_ok=False)
    record = dict(base=BASE, driver_sha256=sha(Path(__file__)), original_sources={}, generated_sources={}, outputs=[])
    for label, checkpoint in [("baseline", "baseline"), ("selected", "replica")]:
        expected = json.loads((EVIDENCE / (checkpoint + ".json")).read_text())["source_sha256"]
        original = {}
        generated = {}
        for name, digest in expected.items():
            if not name.startswith("crates/deadlib-image/png/src/"):
                continue
            data = subprocess.check_output(["git", "show", BASE + ":" + name], cwd=ROOT)
            if label == "selected":
                data = (ROOT / name).read_bytes()
            assert hashlib.sha256(data).hexdigest() == digest, name
            original[name] = digest
            if name.endswith("/zlib.rs"):
                data += WRAPPER
            path = WORK / label / "src" / name.split("/src/", 1)[1]
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            generated[name] = sha(path)
        record["original_sources"][label] = original
        record["generated_sources"][label] = generated
        (WORK / label / "Cargo.toml").write_text(
            f'[package]\nname = "png-setup-{label}"\nversion = "0.1.0"\nedition = "2024"\n'
            '[dependencies]\ndeadlib-image = { path = ' + json.dumps(PNG.parent.as_posix()) + ' }\n')
    source = WORK / "src"
    source.mkdir()
    shutil.copy2(EVIDENCE / "setup.rs", source / "main.rs")
    shutil.copy2(PNG / "examples/support/cpu.rs", source / "cpu.rs")
    settings = tomllib.loads((ROOT / "Cargo.toml").read_text())
    profile = dict(settings["profile"]["release"])
    profile.update(settings["profile"].get("bench", {}))
    lines = "\n".join(key + " = " + json.dumps(value) for key, value in profile.items()
                      if isinstance(value, (str, int, bool)) and key != "inherits")
    (WORK / "Cargo.toml").write_text(
        '[workspace]\nmembers = ["baseline", "selected"]\nresolver = "3"\n'
        '[package]\nname = "png-setup-compare"\nversion = "0.1.0"\nedition = "2024"\n'
        '[dependencies]\nbaseline = { package = "png-setup-baseline", path = "baseline" }\n'
        'selected = { package = "png-setup-selected", path = "selected" }\n'
        '[profile.bench]\n' + lines + '\n')
    command = ["cargo", "build", "--offline", "--manifest-path", str(WORK / "Cargo.toml"),
               "--profile", "bench", "--target-dir", str(WORK / "build"), "--message-format", "json"]
    result = subprocess.run(command, cwd=ROOT, capture_output=True)
    log = EVIDENCE / "replica-build.log"
    log.write_bytes(result.stdout + result.stderr)
    assert result.returncode == 0, result.stderr.decode(errors="replace")
    artifacts = [json.loads(line) for line in result.stdout.splitlines()]
    binary = Path(next(a["executable"] for a in reversed(artifacts)
                       if a.get("reason") == "compiler-artifact" and a.get("executable")))
    saved = WORK / "canonical_bench.exe"
    shutil.copy2(binary, saved)
    record.update(command=command, profile=profile, build_log_sha256=sha(log),
                  harness_sha256=sha(EVIDENCE / "setup.rs"), cpu_sha256=sha(source / "cpu.rs"),
                  executable=str(saved), executable_sha256=sha(saved))
    for run in range(1, 4):
        result = subprocess.run([str(saved)], cwd=ROOT, capture_output=True)
        assert result.returncode == 0, result.stderr.decode(errors="replace")
        log = EVIDENCE / f"replication-run{run}.log"
        log.write_bytes(result.stdout + result.stderr)
        record["outputs"].append(dict(path=log.name, sha256=sha(log)))
        manifest.write_bytes((json.dumps(record, indent=2) + "\n").encode())
        print(f"setup comparison {run} passed", flush=True)


if __name__ == "__main__":
    main()
